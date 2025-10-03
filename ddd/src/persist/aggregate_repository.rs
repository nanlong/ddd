use crate::{
    aggregate::Aggregate,
    domain_event::{BusinessContext, EventEnvelope},
};
use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait AggregateRepository<A>: Send + Sync
where
    A: Aggregate,
{
    async fn load(&self, aggregate_id: &str) -> Result<Option<A>, A::Error>;

    async fn save(
        &self,
        aggregate: &A,
        events: Vec<A::Event>,
        context: BusinessContext,
    ) -> Result<Vec<EventEnvelope<A>>, A::Error>;
}

#[async_trait]
impl<A, T> AggregateRepository<A> for Arc<T>
where
    A: Aggregate,
    T: AggregateRepository<A> + ?Sized,
{
    async fn load(&self, aggregate_id: &str) -> Result<Option<A>, A::Error> {
        (**self).load(aggregate_id).await
    }

    async fn save(
        &self,
        aggregate: &A,
        events: Vec<A::Event>,
        context: BusinessContext,
    ) -> Result<Vec<EventEnvelope<A>>, A::Error> {
        (**self).save(aggregate, events, context).await
    }
}
