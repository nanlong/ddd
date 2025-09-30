use crate::aggregate::Aggregate;
use chrono::{DateTime, Utc};
use std::{slice::Iter, vec::IntoIter};

pub trait DomainEvent {
    fn event_type(&self) -> String;

    fn event_version(&self) -> i64;
}

/// 事件元数据
/// 包含事件的关联信息，如关联ID、因果ID、触发者等
/// 一般来说，这些信息由interface层（如API网关、消息消费者等）的请求上下文中提取
#[derive(Default, Debug, Clone, bon::Builder)]
pub struct Metadata {
    correlation_id: Option<String>,
    causation_id: Option<String>,
    actor_type: Option<String>,
    actor_id: Option<String>,
}

/// 事件信封，包含事件及其元数据
#[derive(Debug, Clone)]
pub struct EventEnvelope<A>
where
    A: Aggregate,
{
    /// 事件唯一ID
    pub event_id: String,
    /// 事件类型
    pub event_type: String,
    /// 事件版本
    pub event_version: i64,
    /// 聚合根ID
    pub aggregate_id: String,
    /// 聚合根类型
    pub aggregate_type: String,
    /// 事件发生时间
    pub occurred_at: DateTime<Utc>,
    /// 关联ID，用于追踪一个请求的所有事件
    pub correlation_id: Option<String>,
    /// 因果ID, 用于标识触发当前事件的上一个事件
    pub causation_id: Option<String>,
    /// 事件触发者类型
    pub actor_type: Option<String>,
    /// 事件触发者ID
    pub actor_id: Option<String>,
    /// 事件数据
    pub event: A::Event,
}

impl<A> EventEnvelope<A>
where
    A: Aggregate,
{
    pub fn new(
        event_id: impl Into<String>,
        aggregate: &A,
        event: A::Event,
        metadata: Metadata,
    ) -> Self {
        Self {
            event_id: event_id.into(),
            event_type: event.event_type(),
            event_version: event.event_version(),
            aggregate_id: aggregate.id().to_string(),
            aggregate_type: A::TYPE.to_string(),
            occurred_at: Utc::now(),
            correlation_id: metadata.correlation_id,
            causation_id: metadata.causation_id,
            actor_type: metadata.actor_type,
            actor_id: metadata.actor_id,
            event,
        }
    }
}

/// 聚合事件集合，按时间顺序排列，加载当前聚合根所有的事件
pub struct AggregateEvents<A>
where
    A: Aggregate,
{
    events: Vec<EventEnvelope<A>>,
}

impl<A> AggregateEvents<A>
where
    A: Aggregate,
{
    pub fn new(events: Vec<EventEnvelope<A>>) -> Self {
        Self { events }
    }

    /// 获取创建者ID（第一个事件的触发者）
    pub fn created_by(&self) -> Option<String> {
        self.events.first().and_then(|e| e.actor_id.clone())
    }

    /// 获取最后修改者ID（最后一个事件的触发者）
    pub fn last_modified_by(&self) -> Option<String> {
        self.events.last().and_then(|e| e.actor_id.clone())
    }

    /// 获取创建时间（第一个事件的发生时间）
    pub fn created_at(&self) -> Option<DateTime<Utc>> {
        self.events.first().map(|e| e.occurred_at)
    }

    /// 获取最后修改时间（最后一个事件的发生时间）
    pub fn last_modified_at(&self) -> Option<DateTime<Utc>> {
        self.events.last().map(|e| e.occurred_at)
    }

    /// 获取事件列表的不可变引用
    pub fn events(&self) -> &[EventEnvelope<A>] {
        &self.events
    }

    /// 获取事件数量
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// 判断是否为空
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// 迭代事件引用（不消费 AggregateEvents）
    pub fn iter(&self) -> Iter<'_, EventEnvelope<A>> {
        self.events.iter()
    }

    /// 迭代并消费 AggregateEvents
    pub fn into_iter(self) -> IntoIter<EventEnvelope<A>> {
        self.events.into_iter()
    }
}
