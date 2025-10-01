use anyhow::Result;
use std::sync::Arc;

/// 事件版本升级器（Upcaster）
pub trait EventUpcaster: Send + Sync {
    type Event;

    fn applies(&self, e: &Self::Event) -> bool;

    fn upcast(&self, e: Self::Event) -> Result<EventUpcasterResult<Self::Event>>;
}

/// 升级结果：单个、新的多个、或丢弃
pub enum EventUpcasterResult<T> {
    One(T),
    Many(Vec<T>),
    Drop,
}

/// 事件升级链：按顺序应用多个 Upcaster
pub struct EventUpcasterChain<T> {
    stages: Vec<Arc<dyn EventUpcaster<Event = T>>>,
}

impl<T> EventUpcasterChain<T> {
    pub fn new() -> Self {
        Self { stages: vec![] }
    }

    /// 添加一个 Upcaster 升级器到链中
    pub fn add<U: EventUpcaster<Event = T> + 'static>(mut self, u: U) -> Self {
        self.stages.push(Arc::new(u));
        self
    }

    /// 对一批事件进行升级，直到不再有升级发生
    pub fn upcast_all(&self, mut events: Vec<T>) -> Result<Vec<T>> {
        loop {
            let (upcasted, has_changes) = self.upcast_once(events)?;
            if !has_changes {
                return Ok(upcasted);
            }
            events = upcasted;
        }
    }

    /// 执行一轮完整的升级，返回升级后的事件列表和是否有变化
    fn upcast_once(&self, events: Vec<T>) -> Result<(Vec<T>, bool)> {
        let mut has_changes = false;

        let upcasted = events
            .into_iter()
            .map(|event| self.upcast_single_event(event, &mut has_changes))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();

        Ok((upcasted, has_changes))
    }

    /// 处理单个事件通过所有升级阶段
    fn upcast_single_event(&self, event: T, has_changes: &mut bool) -> Result<Vec<T>> {
        self.stages.iter().try_fold(vec![event], |events, stage| {
            self.apply_stage(stage, events, has_changes)
        })
    }

    /// 对事件列表应用单个升级器
    fn apply_stage(
        &self,
        stage: &Arc<dyn EventUpcaster<Event = T>>,
        events: Vec<T>,
        has_changes: &mut bool,
    ) -> Result<Vec<T>> {
        let results = events
            .into_iter()
            .map(|event| {
                if stage.applies(&event) {
                    *has_changes = true;
                    stage.upcast(event)
                } else {
                    Ok(EventUpcasterResult::One(event))
                }
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(results
            .into_iter()
            .flat_map(|result| match result {
                EventUpcasterResult::One(e) => vec![e],
                EventUpcasterResult::Many(v) => v,
                EventUpcasterResult::Drop => vec![],
            })
            .collect())
    }
}
