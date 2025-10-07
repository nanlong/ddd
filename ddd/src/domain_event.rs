use crate::aggregate::Aggregate;
use bon::Builder;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{fmt, ops::Deref, slice::Iter, vec::IntoIter};

pub trait DomainEvent:
    Clone + PartialEq + fmt::Debug + Serialize + DeserializeOwned + Send + Sync
{
    fn event_id(&self) -> String;

    fn event_type(&self) -> String;

    fn event_version(&self) -> usize;

    fn aggregate_version(&self) -> usize;
}

/// 元数据
#[derive(Builder, Default, Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    aggregate_id: String,
    aggregate_type: String,
    occurred_at: DateTime<Utc>,
}

impl Metadata {
    pub fn aggregate_id(&self) -> &str {
        &self.aggregate_id
    }

    pub fn aggregate_type(&self) -> &str {
        &self.aggregate_type
    }

    pub fn occurred_at(&self) -> &DateTime<Utc> {
        &self.occurred_at
    }
}

/// 业务上下文信息
#[derive(Builder, Default, Debug, Clone, Serialize, Deserialize)]
pub struct BusinessContext {
    correlation_id: Option<String>,
    causation_id: Option<String>,
    actor_type: Option<String>,
    actor_id: Option<String>,
}

impl BusinessContext {
    pub fn correlation_id(&self) -> Option<&str> {
        self.correlation_id.as_deref()
    }

    pub fn causation_id(&self) -> Option<&str> {
        self.causation_id.as_deref()
    }

    pub fn actor_type(&self) -> Option<&str> {
        self.actor_type.as_deref()
    }

    pub fn actor_id(&self) -> Option<&str> {
        self.actor_id.as_deref()
    }
}

/// 事件信封，包含事件及其元数据
#[derive(Debug, Clone)]
pub struct EventEnvelope<A>
where
    A: Aggregate,
{
    pub metadata: Metadata,
    pub payload: A::Event,
    pub context: BusinessContext,
}

impl<A> EventEnvelope<A>
where
    A: Aggregate,
{
    pub fn new(aggregate_id: &A::Id, payload: A::Event, context: BusinessContext) -> Self {
        let metadata = Metadata::builder()
            .aggregate_id(aggregate_id.to_string())
            .aggregate_type(A::TYPE.to_string())
            .occurred_at(Utc::now())
            .build();

        Self {
            metadata,
            payload,
            context,
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
        self.events.first().and_then(|e| e.context.actor_id.clone())
    }

    /// 获取最后修改者ID（最后一个事件的触发者）
    pub fn last_modified_by(&self) -> Option<String> {
        self.events.last().and_then(|e| e.context.actor_id.clone())
    }

    /// 获取创建时间（第一个事件的发生时间）
    pub fn created_at(&self) -> Option<DateTime<Utc>> {
        self.events.first().map(|e| e.metadata.occurred_at)
    }

    /// 获取最后修改时间（最后一个事件的发生时间）
    pub fn last_modified_at(&self) -> Option<DateTime<Utc>> {
        self.events.last().map(|e| e.metadata.occurred_at)
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
}

impl<A> IntoIterator for AggregateEvents<A>
where
    A: Aggregate,
{
    type Item = EventEnvelope<A>;
    type IntoIter = IntoIter<EventEnvelope<A>>;

    fn into_iter(self) -> Self::IntoIter {
        self.events.into_iter()
    }
}

impl<'a, A> IntoIterator for &'a AggregateEvents<A>
where
    A: Aggregate,
{
    type Item = &'a EventEnvelope<A>;
    type IntoIter = Iter<'a, EventEnvelope<A>>;

    fn into_iter(self) -> Self::IntoIter {
        self.events.iter()
    }
}

impl<A> Deref for AggregateEvents<A>
where
    A: Aggregate,
{
    type Target = [EventEnvelope<A>];

    fn deref(&self) -> &Self::Target {
        &self.events
    }
}
