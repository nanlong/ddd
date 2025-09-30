use crate::{
    aggregate::Aggregate,
    event::{AggregateEvents, EventEnvelope, Metadata},
};
use async_trait::async_trait;

#[async_trait]
pub trait Repository<A>: Send + Sync
where
    A: Aggregate,
{
    async fn load_events(&self, aggregate_id: &A::Id) -> Result<AggregateEvents<A>, A::Error>;

    async fn load_aggregate(&self, aggregate_id: &A::Id) -> Result<Option<A>, A::Error>;

    async fn commit(
        &self,
        aggregate: &A,
        events: Vec<A::Event>,
        metadata: Metadata,
    ) -> Result<Vec<EventEnvelope<A>>, A::Error>;
}
