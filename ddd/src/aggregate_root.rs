use crate::{
    aggregate::Aggregate,
    domain_event::{EventEnvelope, Metadata},
    repository::Repository,
};
use anyhow::Result;
use std::marker::PhantomData;

pub struct AggregateRoot<A, R>
where
    A: Aggregate,
    R: Repository<A>,
{
    repo: R,
    _marker: std::marker::PhantomData<A>,
}

impl<A, R> AggregateRoot<A, R>
where
    A: Aggregate,
    R: Repository<A>,
{
    pub fn new(repo: R) -> Self {
        Self {
            repo,
            _marker: PhantomData,
        }
    }

    pub async fn execute(
        &self,
        aggregate_id: &str,
        command: A::Command,
        metadata: Metadata,
    ) -> Result<Vec<EventEnvelope<A>>> {
        // 从仓库加载聚合
        let loaded = self.repo.load_aggregate(aggregate_id).await.map_err(|e| {
            anyhow::anyhow!(
                "Failed to load aggregate {} with id {}: {}",
                A::TYPE,
                aggregate_id,
                e
            )
        })?;

        // 如果不存在则创建新的聚合实例
        let mut aggregate = match loaded {
            Some(aggregate) => aggregate,
            None => A::new(aggregate_id),
        };

        // 执行命令
        let events = aggregate.execute(command).await.map_err(|e| {
            anyhow::anyhow!(
                "Failed to execute command on aggregate {} with id {}: {}",
                A::TYPE,
                aggregate_id,
                e
            )
        })?;

        // 应用所有新生成的事件到聚合状态
        for event in events.iter() {
            aggregate.apply(&event);
        }

        // 保存聚合状态和未提交的事件
        let event_envelopes = self
            .repo
            .commit(&aggregate, events, metadata)
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to save aggregate {} with id {}: {}",
                    A::TYPE,
                    aggregate_id,
                    e
                )
            })?;

        Ok(event_envelopes)
    }
}
