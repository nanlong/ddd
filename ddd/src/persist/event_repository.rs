use crate::{
    aggregate::Aggregate,
    domain_event::{AggregateEvents, EventEnvelope},
    persist::SerializedEvent,
};
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait EventRepository: Send + Sync {
    async fn get_events<A: Aggregate>(&self, aggregate_id: &str) -> Result<Vec<SerializedEvent>>;

    async fn get_last_events<A: Aggregate>(
        &self,
        aggregate_id: &str,
        last_version: usize,
    ) -> Result<Vec<SerializedEvent>>;

    async fn save(&self, events: &[SerializedEvent]) -> Result<()>;
}

#[async_trait]
pub trait EventRepositoryExt: EventRepository {
    async fn get_aggregate_events<A: Aggregate>(
        &self,
        aggregate_id: &str,
    ) -> Result<AggregateEvents<A>> {
        let events = self
            .get_events::<A>(aggregate_id)
            .await?
            .iter()
            .map(|e| EventEnvelope::<A>::try_from(e))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(AggregateEvents::new(events))
    }
}

#[async_trait]
impl<T> EventRepository for Arc<T>
where
    T: EventRepository + ?Sized,
{
    async fn get_events<A: Aggregate>(&self, aggregate_id: &str) -> Result<Vec<SerializedEvent>> {
        (**self).get_events::<A>(aggregate_id).await
    }

    async fn get_last_events<A: Aggregate>(
        &self,
        aggregate_id: &str,
        last_version: usize,
    ) -> Result<Vec<SerializedEvent>> {
        (**self)
            .get_last_events::<A>(aggregate_id, last_version)
            .await
    }

    async fn save(&self, events: &[SerializedEvent]) -> Result<()> {
        (**self).save(events).await
    }
}

#[async_trait]
impl<T> EventRepositoryExt for T where T: EventRepository + ?Sized {}
