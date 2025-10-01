use crate::aggregate::Aggregate;
use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait EventRepository: Send + Sync {
    type SerializedEvent;

    async fn get_events<A: Aggregate>(
        &self,
        aggregate_id: &str,
    ) -> Result<Vec<Self::SerializedEvent>>;

    async fn get_last_events<A: Aggregate>(
        &self,
        aggregate_id: &str,
        last_version: usize,
    ) -> Result<Vec<Self::SerializedEvent>>;

    fn commit<A: Aggregate>(&self, events: &[Self::SerializedEvent]) -> Result<()>;
}
