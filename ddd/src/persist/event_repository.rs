use crate::aggregate::Aggregate;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait EventRepository: Send + Sync {
    type SerializedEvent: Send + Sync;

    async fn get_events<A: Aggregate>(
        &self,
        aggregate_id: &str,
    ) -> Result<Vec<Self::SerializedEvent>>;

    async fn get_last_events<A: Aggregate>(
        &self,
        aggregate_id: &str,
        last_version: usize,
    ) -> Result<Vec<Self::SerializedEvent>>;

    async fn save<A: Aggregate>(&self, events: &[Self::SerializedEvent]) -> Result<()>;
}

#[async_trait]
impl<T> EventRepository for Arc<T>
where
    T: EventRepository + ?Sized,
{
    type SerializedEvent = T::SerializedEvent;

    async fn get_events<A: Aggregate>(
        &self,
        aggregate_id: &str,
    ) -> Result<Vec<Self::SerializedEvent>> {
        (**self).get_events::<A>(aggregate_id).await
    }

    async fn get_last_events<A: Aggregate>(
        &self,
        aggregate_id: &str,
        last_version: usize,
    ) -> Result<Vec<Self::SerializedEvent>> {
        (**self)
            .get_last_events::<A>(aggregate_id, last_version)
            .await
    }

    async fn save<A: Aggregate>(&self, events: &[Self::SerializedEvent]) -> Result<()> {
        (**self).save::<A>(events).await
    }
}
