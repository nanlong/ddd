use crate::{
    aggregate::Aggregate,
    domain_event::{BusinessContext, EventEnvelope},
};
use async_trait::async_trait;

#[async_trait]
pub trait AggragateRepository<A>: Send + Sync
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
