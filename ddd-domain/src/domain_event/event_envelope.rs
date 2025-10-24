use crate::aggregate::Aggregate;
use chrono::Utc;

use super::business_context::BusinessContext;
use super::metadata::Metadata;

/// 事件信封，包含事件载荷、元数据与业务上下文
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
