use crate::{aggregate::Aggregate, error::DomainResult as Result, persist::SerializedSnapshot};
use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait SnapshotRepository: Send + Sync {
    fn snapshot_policy(&self) -> SnapshotPolicy {
        SnapshotPolicy::Every(10)
    }

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
    fn snapshot_policy(&self) -> SnapshotPolicy {
        (**self).snapshot_policy()
    }

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotPolicy {
    Never,
    Every(usize),
}

impl SnapshotPolicy {
    pub fn should_snapshot(&self, version: usize) -> bool {
        match self {
            SnapshotPolicy::Never => false,
            SnapshotPolicy::Every(interval) => {
                let interval = (*interval).max(1);
                version > 0 && version % interval == 0
            }
        }
    }
}
