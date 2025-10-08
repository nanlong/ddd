use crate::{
    aggregate::Aggregate,
    domain_event::{BusinessContext, EventEnvelope},
    entiry::Entity,
    persist::AggregateRepository,
};
use std::marker::PhantomData;

pub struct AggregateRoot<A, R>
where
    A: Aggregate,
    R: AggregateRepository<A>,
{
    repo: R,
    _marker: PhantomData<A>,
}

impl<A, R> AggregateRoot<A, R>
where
    A: Aggregate,
    R: AggregateRepository<A>,
{
    pub fn new(repo: R) -> Self {
        Self {
            repo,
            _marker: PhantomData,
        }
    }

    pub async fn execute(
        &self,
        aggregate_id: &A::Id,
        command: A::Command,
        context: BusinessContext,
    ) -> Result<Vec<EventEnvelope<A>>, A::Error> {
        // 从仓库加载聚合
        let loaded = self.repo.load(aggregate_id.as_ref()).await?;

        // 如果不存在则创建新的聚合实例
        let mut aggregate = match loaded {
            Some(aggregate) => aggregate,
            None => <A as Entity>::new(aggregate_id.clone()),
        };

        // 执行命令
        let events = aggregate.execute(command)?;

        // 应用所有新生成的事件到聚合状态
        for event in &events {
            aggregate.apply(event);
        }

        // 保存聚合状态和未提交的事件
        let event_envelopes = self.repo.save(&aggregate, events, context).await?;

        Ok(event_envelopes)
    }
}
