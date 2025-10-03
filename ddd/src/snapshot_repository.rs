use crate::aggregate::Aggregate;
use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait SnapshotRepository: Send + Sync {
    type SerializedSnapshot;

    async fn get_snapshot<A: Aggregate>(
        &self,
        aggregate_id: &str,
        version: Option<usize>,
    ) -> Result<Option<Self::SerializedSnapshot>>;

    fn save<A: Aggregate>(&self, aggregate: &A) -> Result<()>;
}
