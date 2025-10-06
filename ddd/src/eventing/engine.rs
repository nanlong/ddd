use super::handler::HandledEventType;
use super::{EventBus, EventDeliverer, EventHandler, EventReclaimer};
use crate::persist::SerializedEvent;
use async_trait::async_trait;
use bon::Builder;
use futures_util::{StreamExt, stream};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::task::JoinHandle;
use tokio::time::{self, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

// 导入由 bon::Builder 生成的 typestate 模块与状态转换别名
use self::event_engine_builder::{IsUnset, SetRegistry, State as BuilderState};

/// EventEngine：
/// - 周期性从 Deliverer/Reclaimer 拉取事件并发布到 Bus
/// - 订阅 Bus 的事件流，分发到匹配的 Handler，并发处理
#[derive(Builder)]
pub struct EventEngine {
    event_bus: Arc<dyn EventBus>,
    event_deliverer: Arc<dyn EventDeliverer>,
    event_reclaimer: Arc<dyn EventReclaimer>,
    #[builder(setters(vis = "pub(crate)"))]
    registry: HandlerRegistry,
    #[builder(default)]
    config: EventEngineConfig,
}

impl<S: BuilderState> EventEngineBuilder<S> {
    pub fn event_handlers(
        self,
        handlers: Vec<Arc<dyn EventHandler>>,
    ) -> EventEngineBuilder<SetRegistry<S>>
    where
        <S as BuilderState>::Registry: IsUnset,
    {
        self.registry(HandlerRegistry::new(handlers))
    }
}

impl EventEngine {
    /// 启动事件引擎，返回可用于关闭/等待的句柄
    pub fn start(self: Arc<Self>) -> EngineHandle {
        let token = CancellationToken::new();
        let mut tasks: Vec<JoinHandle<()>> = Vec::with_capacity(3);

        // deliver worker（周期任务）
        {
            let bus = self.event_bus.clone();
            let deliverer = self.event_deliverer.clone();
            let marker = DelivererMarker::new(deliverer.clone());
            let interval = self.config.deliver_interval;

            tasks.push(Self::spawn_periodic(token.clone(), interval, move || {
                let bus = bus.clone();
                let deliverer = deliverer.clone();
                let marker = marker.clone();
                async move {
                    if let Ok(events) = deliverer.fetch_events().await {
                        Self::publish_and_mark(&bus, &marker, events).await;
                    }
                }
            }));
        }

        // reclaim worker（周期任务）
        {
            let bus = self.event_bus.clone();
            let reclaimer = self.event_reclaimer.clone();
            let marker = ReclaimerMarker::new(reclaimer.clone());
            let interval = self.config.reclaim_interval;

            tasks.push(Self::spawn_periodic(token.clone(), interval, move || {
                let bus = bus.clone();
                let reclaimer = reclaimer.clone();
                let marker = marker.clone();
                async move {
                    if let Ok(events) = reclaimer.fetch_events().await {
                        Self::publish_and_mark(&bus, &marker, events).await;
                    }
                }
            }));
        }

        // subscribe worker（长循环）
        tasks.push(tokio::spawn(Self::subscribe_loop(
            self.clone(),
            token.clone(),
        )));

        EngineHandle { token, tasks }
    }

    fn spawn_periodic<F, Fut>(
        token: CancellationToken,
        interval: Duration,
        mut f: F,
    ) -> JoinHandle<()>
    where
        F: FnMut() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        tokio::spawn(async move {
            let mut ticker = time::interval(interval);
            ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    _ = token.cancelled() => break,
                    _ = ticker.tick() => f().await,
                }
            }
        })
    }

    async fn publish_and_mark(
        bus: &Arc<dyn EventBus>,
        marker: &impl EventBatchMarker,
        events: Vec<SerializedEvent>,
    ) {
        if events.is_empty() {
            return;
        }

        match bus.publish_batch(&events).await {
            Ok(()) => {
                let refs: Vec<&SerializedEvent> = events.iter().collect();
                marker.mark_success(&refs).await;
            }
            Err(_batch_err) => {
                for ev in &events {
                    match bus.publish(ev).await {
                        Ok(()) => {
                            marker.mark_success(&[ev]).await;
                        }
                        Err(e) => {
                            let reason = e.to_string();
                            marker.mark_failure(&[ev], &reason).await;
                        }
                    }
                }
            }
        }
    }

    async fn subscribe_loop(self: Arc<Self>, token: CancellationToken) {
        let mut stream = self.event_bus.subscribe().await;
        let registry = self.registry.clone();
        let concurrency = self.config.handler_concurrency;
        let reclaimer = self.event_reclaimer.clone();

        loop {
            tokio::select! {
                _ = token.cancelled() => {
                    break;
                }
                maybe_event = stream.next() => {
                    match maybe_event {
                        Some(event) => {
                            let merged = registry.matching(event.event_type());
                            if merged.is_empty() { continue; }
                            let tasks = merged.into_iter();
                            let reclaimer_for_stream = reclaimer.clone();

                            stream::iter(tasks)
                                .for_each_concurrent(Some(concurrency), move |h| {
                                    let ev = event.clone();
                                    let reclaimer = reclaimer_for_stream.clone();
                                    async move {
                                        if let Err(err) = h.handle(&ev).await {
                                            let _ = reclaimer
                                                .mark_handler_failed(h.handler_name(), &[&ev], &err.to_string())
                                                .await;
                                        }
                                    }
                                })
                                .await;
                        }
                        None => {
                            break;
                        }
                    }
                }
            }
        }
    }
}

// 自定义 Builder 方法：接收 handlers，内部转换为 HandlerRegistry 并设置到 builder 的 registry 字段。
// 注意：受 typestate 限制，仅当 `registry` 尚未设置时可调用。
// 若已设置 `registry`，编译器会报错提示重复设置。
// 正确的做法是：链式调用一次 `event_handlers(...)` 即可。

#[derive(Clone, Default)]
struct HandlerRegistry {
    by_type: HashMap<String, Vec<Arc<dyn EventHandler>>>,
    all: Vec<Arc<dyn EventHandler>>,
}

impl HandlerRegistry {
    fn new(handlers: Vec<Arc<dyn EventHandler>>) -> Self {
        let mut by_type: HashMap<String, Vec<Arc<dyn EventHandler>>> = HashMap::new();
        let mut all: Vec<Arc<dyn EventHandler>> = Vec::new();

        for h in handlers {
            match h.handled_event_type() {
                HandledEventType::All => all.push(h),
                HandledEventType::One(t) => {
                    by_type.entry(t).or_default().push(h);
                }
                HandledEventType::Many(ts) => {
                    for t in ts {
                        by_type.entry(t).or_default().push(h.clone());
                    }
                }
            }
        }

        Self { by_type, all }
    }

    fn matching(&self, event_type: &str) -> Vec<Arc<dyn EventHandler>> {
        let mut merged: Vec<Arc<dyn EventHandler>> = Vec::new();
        if let Some(list) = self.by_type.get(event_type) {
            merged.extend(list.iter().cloned());
        }
        merged.extend(self.all.iter().cloned());
        merged
    }
}

#[async_trait]
trait EventBatchMarker: Send + Sync {
    async fn mark_success(&self, events: &[&SerializedEvent]);
    async fn mark_failure(&self, events: &[&SerializedEvent], reason: &str);
}

#[derive(Clone)]
struct DelivererMarker {
    inner: Arc<dyn EventDeliverer>,
}

impl DelivererMarker {
    fn new(inner: Arc<dyn EventDeliverer>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl EventBatchMarker for DelivererMarker {
    async fn mark_success(&self, events: &[&SerializedEvent]) {
        let _ = self.inner.mark_delivered(events).await;
    }

    async fn mark_failure(&self, events: &[&SerializedEvent], reason: &str) {
        let _ = self.inner.mark_failed(events, reason).await;
    }
}

#[derive(Clone)]
struct ReclaimerMarker {
    inner: Arc<dyn EventReclaimer>,
}

impl ReclaimerMarker {
    fn new(inner: Arc<dyn EventReclaimer>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl EventBatchMarker for ReclaimerMarker {
    async fn mark_success(&self, events: &[&SerializedEvent]) {
        let _ = self.inner.mark_reclaimed(events).await;
    }

    async fn mark_failure(&self, events: &[&SerializedEvent], reason: &str) {
        let _ = self.inner.mark_failed(events, reason).await;
    }
}

/// 事件引擎配置
#[derive(Clone, Copy, Debug)]
pub struct EventEngineConfig {
    /// Outbox -> Bus 的推送间隔
    pub deliver_interval: Duration,
    /// 补偿投递的间隔
    pub reclaim_interval: Duration,
    /// 单事件的处理并发（同一事件广播给多个 handler）
    pub handler_concurrency: usize,
}

impl Default for EventEngineConfig {
    fn default() -> Self {
        Self {
            deliver_interval: Duration::from_secs(10),
            reclaim_interval: Duration::from_secs(60),
            handler_concurrency: 8,
        }
    }
}

/// 引擎运行句柄：用于优雅关闭与等待任务结束
pub struct EngineHandle {
    token: CancellationToken,
    tasks: Vec<JoinHandle<()>>,
}

impl EngineHandle {
    pub fn shutdown(&self) {
        self.token.cancel();
    }

    pub async fn join(mut self) {
        let tasks = std::mem::take(&mut self.tasks);

        for t in tasks {
            let _ = t.await;
        }
    }
}

impl Drop for EngineHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}
