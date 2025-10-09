//! 聚合根编排器（AggregateRoot）
//!
//! 封装从“加载聚合 → 执行命令 → 应用事件 → 持久化事件”的标准流程，
//! 以仓储实现（`AggregateRepository`）为依赖，便于在应用层直接调用。
//!
use crate::{
    aggregate::Aggregate,
    domain_event::{BusinessContext, EventEnvelope},
    entiry::Entity,
    persist::AggregateRepository,
};
use std::marker::PhantomData;

/// 面向应用层的聚合根编排器。
///
/// - `A`：聚合类型（实现 `Aggregate`）
/// - `R`：聚合仓储（实现 `AggregateRepository<A>`）
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
    /// 创建编排器实例
    pub fn new(repo: R) -> Self {
        Self {
            repo,
            _marker: PhantomData,
        }
    }

    /// 执行聚合命令：
    /// 1. 若未持久化则创建新聚合；
    /// 2. 执行命令得到新事件；
    /// 3. 应用事件到聚合状态；
    /// 4. 调用仓储持久化并返回事件信封。
    pub async fn execute(
        &self,
        aggregate_id: &A::Id,
        command: A::Command,
        context: BusinessContext,
    ) -> Result<Vec<EventEnvelope<A>>, A::Error> {
        // 如果不存在则创建新的聚合实例
        let mut aggregate = match self.repo.load(&aggregate_id.to_string()).await? {
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
