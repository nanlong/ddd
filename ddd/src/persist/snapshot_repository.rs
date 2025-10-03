use crate::aggregate::Aggregate;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

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

#[async_trait]
impl<T> SnapshotRepository for Arc<T>
where
    T: SnapshotRepository + ?Sized,
{
    type SerializedSnapshot = T::SerializedSnapshot;

    async fn get_snapshot<A: Aggregate>(
        &self,
        aggregate_id: &str,
        version: Option<usize>,
    ) -> Result<Option<Self::SerializedSnapshot>> {
        (**self).get_snapshot::<A>(aggregate_id, version).await
    }

    fn save<A: Aggregate>(&self, aggregate: &A) -> Result<()> {
        (**self).save::<A>(aggregate)
    }
}
