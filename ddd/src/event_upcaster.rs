use crate::{error::DomainResult as Result, persist::SerializedEvent};
use std::sync::Arc;

/// 事件版本升级器（Upcaster）
pub trait EventUpcaster: Send + Sync {
    fn applies(&self, event_type: &str, event_version: usize) -> bool;

    fn upcast(&self, event: SerializedEvent) -> Result<EventUpcasterResult>;
}

impl<T> EventUpcaster for Arc<T>
where
    T: EventUpcaster + ?Sized,
{
    fn applies(&self, event_type: &str, event_version: usize) -> bool {
        (**self).applies(event_type, event_version)
    }

    fn upcast(&self, event: SerializedEvent) -> Result<EventUpcasterResult> {
        (**self).upcast(event)
    }
}

impl<T> EventUpcaster for Box<T>
where
    T: EventUpcaster + ?Sized,
{
    fn applies(&self, event_type: &str, event_version: usize) -> bool {
        (**self).applies(event_type, event_version)
    }

    fn upcast(&self, event: SerializedEvent) -> Result<EventUpcasterResult> {
        (**self).upcast(event)
    }
}

/// 升级结果：单个、新的多个、或丢弃
#[allow(clippy::large_enum_variant)]
pub enum EventUpcasterResult {
    One(SerializedEvent),
    Many(Vec<SerializedEvent>),
    Drop,
}

/// 事件升级链：按顺序应用多个 Upcaster
pub struct EventUpcasterChain {
    stages: Vec<Arc<dyn EventUpcaster>>,
}

impl Default for EventUpcasterChain {
    fn default() -> Self {
        Self::new()
    }
}

impl EventUpcasterChain {
    pub fn new() -> Self {
        Self { stages: vec![] }
    }

    /// 添加一个 Upcaster 升级器到链中
    pub fn push<U>(mut self, u: U) -> Self
    where
        U: EventUpcaster + 'static,
    {
        self.stages.push(Arc::new(u));
        self
    }

    /// 对一批事件进行升级，直到不再有升级发生
    pub fn upcast_all(&self, mut events: Vec<SerializedEvent>) -> Result<Vec<SerializedEvent>> {
        loop {
            let (upcasted, has_changes) = self.upcast_once(events)?;
            if !has_changes {
                return Ok(upcasted);
            }
            events = upcasted;
        }
    }

    /// 执行一轮完整的升级，返回升级后的事件列表和是否有变化
    fn upcast_once(&self, events: Vec<SerializedEvent>) -> Result<(Vec<SerializedEvent>, bool)> {
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
    fn upcast_single_event(
        &self,
        event: SerializedEvent,
        has_changes: &mut bool,
    ) -> Result<Vec<SerializedEvent>> {
        self.stages.iter().try_fold(vec![event], |events, stage| {
            self.apply_stage(stage, events, has_changes)
        })
    }

    /// 对事件列表应用单个升级器
    fn apply_stage(
        &self,
        stage: &Arc<dyn EventUpcaster>,
        events: Vec<SerializedEvent>,
        has_changes: &mut bool,
    ) -> Result<Vec<SerializedEvent>> {
        let results = events
            .into_iter()
            .map(|event| {
                if stage.applies(event.event_type(), event.event_version()) {
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

// 支持从具体类型 U（实现 EventUpcaster）迭代器收集
impl<U> FromIterator<U> for EventUpcasterChain
where
    U: EventUpcaster + 'static,
{
    fn from_iter<T: IntoIterator<Item = U>>(iter: T) -> Self {
        Self {
            stages: iter
                .into_iter()
                .map(|u| -> Arc<dyn EventUpcaster> { Arc::new(u) })
                .collect(),
        }
    }
}

impl<U> Extend<U> for EventUpcasterChain
where
    U: EventUpcaster + 'static,
{
    fn extend<T: IntoIterator<Item = U>>(&mut self, iter: T) {
        self.stages.extend(
            iter.into_iter()
                .map(|u| -> Arc<dyn EventUpcaster> { Arc::new(u) }),
        );
    }
}
