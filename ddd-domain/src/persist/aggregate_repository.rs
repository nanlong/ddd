//! 聚合仓储组合实现
//!
//! 基于事件溯源（Event Store）与快照（Snapshot）的通用聚合仓储实现，
//! 通过事件上抬链在重建过程中完成旧事件兼容。
//!
use crate::error::DomainError;
use crate::{
    aggregate::Aggregate,
    domain_event::{BusinessContext, EventEnvelope},
    entity::Entity,
    event_upcaster::EventUpcasterChain,
    persist::{EventRepository, SnapshotRepository, deserialize_events, serialize_events},
};
use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait AggregateRepository<A>: Send + Sync
where
    A: Aggregate,
{
    async fn load(&self, aggregate_id: &str) -> Result<Option<A>, A::Error>;

    async fn save(
        &self,
        aggregate: &A,
        events: Vec<A::Event>,
        context: BusinessContext,
    ) -> Result<Vec<EventEnvelope<A>>, A::Error>;
}

#[async_trait]
impl<A, T> AggregateRepository<A> for Arc<T>
where
    A: Aggregate,
    T: AggregateRepository<A> + ?Sized,
{
    async fn load(&self, aggregate_id: &str) -> Result<Option<A>, A::Error> {
        (**self).load(aggregate_id).await
    }

    async fn save(
        &self,
        aggregate: &A,
        events: Vec<A::Event>,
        context: BusinessContext,
    ) -> Result<Vec<EventEnvelope<A>>, A::Error> {
        (**self).save(aggregate, events, context).await
    }
}

/// 基于事件存储的通用聚合仓储实现。
/// - 使用 `EventRepository` 读取/保存事件
/// - 在重建聚合时通过 `EventUpcasterChain` 对事件进行上抬
pub struct EventStoreAggregateRepository<A, E>
where
    A: Aggregate,
    E: EventRepository,
{
    event_repo: Arc<E>,
    upcaster_chain: Arc<EventUpcasterChain>,
    _marker: std::marker::PhantomData<A>,
}

impl<A, E> EventStoreAggregateRepository<A, E>
where
    A: Aggregate,
    E: EventRepository,
{
    pub fn new(event_repo: Arc<E>, upcaster_chain: Arc<EventUpcasterChain>) -> Self {
        Self {
            event_repo,
            upcaster_chain,
            _marker: std::marker::PhantomData,
        }
    }
}

#[async_trait]
impl<A, E> AggregateRepository<A> for EventStoreAggregateRepository<A, E>
where
    A: Aggregate,
    E: EventRepository + Send + Sync,
    A::Error: From<DomainError> + Send + Sync,
{
    async fn load(&self, aggregate_id: &str) -> Result<Option<A>, A::Error> {
        let serialized = self
            .event_repo
            .get_events::<A>(aggregate_id)
            .await
            .map_err(A::Error::from)?;

        if serialized.is_empty() {
            return Ok(None);
        }

        let envelopes =
            deserialize_events::<A>(&self.upcaster_chain, serialized).map_err(A::Error::from)?;

        let id: A::Id = match aggregate_id.parse() {
            Ok(id) => id,
            Err(_) => {
                return Err(A::Error::from(DomainError::InvalidAggregateId(
                    aggregate_id.to_string(),
                )));
            }
        };

        let mut aggregate = <A as Entity>::new(id, 0);

        for envelope in envelopes.iter() {
            aggregate.apply(&envelope.payload);
        }

        Ok(Some(aggregate))
    }

    async fn save(
        &self,
        aggregate: &A,
        events: Vec<A::Event>,
        context: BusinessContext,
    ) -> Result<Vec<EventEnvelope<A>>, A::Error> {
        let envelopes: Vec<EventEnvelope<A>> = events
            .into_iter()
            .map(|e| EventEnvelope::new(aggregate.id(), e, context.clone()))
            .collect();

        if envelopes.is_empty() {
            return Ok(envelopes);
        }

        let serialized = serialize_events(&envelopes).map_err(A::Error::from)?;

        self.event_repo
            .save(serialized)
            .await
            .map_err(A::Error::from)?;

        Ok(envelopes)
    }
}

/// 基于事件存储 + 快照 的通用聚合仓储实现。
/// - 优先使用 `SnapshotRepository` 恢复最近快照
/// - 然后加载快照版本之后的增量事件并上抬（Upcast）重放
pub struct SnapshottingAggregateRepository<A, E, S>
where
    A: Aggregate,
    E: EventRepository,
    S: SnapshotRepository,
{
    event_repo: Arc<E>,
    snapshot_repo: Arc<S>,
    upcaster_chain: Arc<EventUpcasterChain>,
    _marker: std::marker::PhantomData<A>,
}

impl<A, E, S> SnapshottingAggregateRepository<A, E, S>
where
    A: Aggregate,
    E: EventRepository,
    S: SnapshotRepository,
{
    pub fn new(
        event_repo: Arc<E>,
        snapshot_repo: Arc<S>,
        upcaster_chain: Arc<EventUpcasterChain>,
    ) -> Self {
        Self {
            event_repo,
            snapshot_repo,
            upcaster_chain,
            _marker: std::marker::PhantomData,
        }
    }
}

#[async_trait]
impl<A, E, S> AggregateRepository<A> for SnapshottingAggregateRepository<A, E, S>
where
    A: Aggregate,
    E: EventRepository + Send + Sync,
    S: SnapshotRepository + Send + Sync,
    A::Error: From<DomainError> + Send + Sync,
{
    async fn load(&self, aggregate_id: &str) -> Result<Option<A>, A::Error> {
        // 1. 先尝试从快照还原
        if let Some(snapshot) = self
            .snapshot_repo
            .get_snapshot::<A>(aggregate_id, None)
            .await
            .map_err(A::Error::from)?
        {
            let mut aggregate = snapshot.to_aggregate::<A>().map_err(A::Error::from)?;
            let snapshot_version = snapshot.aggregate_version();

            // 2. 加载快照之后的增量事件
            let incremental = self
                .event_repo
                .get_last_events::<A>(aggregate_id, snapshot_version)
                .await
                .map_err(A::Error::from)?;

            let envelopes = deserialize_events::<A>(&self.upcaster_chain, incremental)
                .map_err(A::Error::from)?;

            for envelope in envelopes.iter() {
                aggregate.apply(&envelope.payload);
            }

            return Ok(Some(aggregate));
        }

        // 3. 没有快照，则从所有事件重建
        let serialized = self
            .event_repo
            .get_events::<A>(aggregate_id)
            .await
            .map_err(A::Error::from)?;

        if serialized.is_empty() {
            return Ok(None);
        }

        let envelopes =
            deserialize_events::<A>(&self.upcaster_chain, serialized).map_err(A::Error::from)?;

        let id: A::Id = match aggregate_id.parse() {
            Ok(id) => id,
            Err(_) => {
                return Err(A::Error::from(DomainError::InvalidAggregateId(
                    aggregate_id.to_string(),
                )));
            }
        };

        let mut aggregate = <A as Entity>::new(id, 0);

        for envelope in envelopes.iter() {
            aggregate.apply(&envelope.payload);
        }

        Ok(Some(aggregate))
    }

    async fn save(
        &self,
        aggregate: &A,
        events: Vec<A::Event>,
        context: BusinessContext,
    ) -> Result<Vec<EventEnvelope<A>>, A::Error> {
        let envelopes: Vec<EventEnvelope<A>> = events
            .into_iter()
            .map(|e| EventEnvelope::new(aggregate.id(), e, context.clone()))
            .collect();

        if envelopes.is_empty() {
            return Ok(envelopes);
        }

        let serialized = serialize_events(&envelopes).map_err(A::Error::from)?;

        self.event_repo
            .save(serialized)
            .await
            .map_err(A::Error::from)?;

        self.snapshot_repo
            .save(aggregate)
            .await
            .map_err(A::Error::from)?;

        Ok(envelopes)
    }
}
