use crate::{aggregate::Aggregate, error::DomainResult as Result, persist::SerializedSnapshot};
use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait SnapshotRepository: Send + Sync {
    async fn get_snapshot<A: Aggregate>(
        &self,
        aggregate_id: &str,
        version: Option<usize>,
    ) -> Result<Option<SerializedSnapshot>>;

    async fn save<A: Aggregate>(&self, aggregate: &A) -> Result<()>;
}

#[async_trait]
impl<T> SnapshotRepository for Arc<T>
where
    T: SnapshotRepository + ?Sized,
{
    async fn get_snapshot<A: Aggregate>(
        &self,
        aggregate_id: &str,
        version: Option<usize>,
    ) -> Result<Option<SerializedSnapshot>> {
        (**self).get_snapshot::<A>(aggregate_id, version).await
    }

    async fn save<A: Aggregate>(&self, aggregate: &A) -> Result<()> {
        (**self).save::<A>(aggregate).await
    }
}
