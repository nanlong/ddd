//! 持久化与事件溯源（persist）
//!
//! 定义事件仓储、快照仓储及其通用组合实现，支持：
//! - 事件持久化与按聚合查询（`EventRepository`）；
//! - 快照读写与策略（`SnapshotRepository`/`SnapshotPolicy`）；
//! - 事件上抬（Upcast）与反序列化（`deserialize_events`）；
//! - 纯事件或事件+快照的聚合仓储实现（`EventStoreAggregateRepository`、`SnapshottingAggregateRepository`）。
//!
//! 该模块聚焦协议与装配逻辑，具体存储后端（如 Postgres）由上层提供实现并注入。
//!
mod aggregate_repository;
mod event_repository;
mod serialized_event;
mod serialized_snapshot;
mod snapshot_repository;

pub use aggregate_repository::{
    AggregateRepository, EventStoreAggregateRepository, SnapshottingAggregateRepository,
};
pub use event_repository::{EventRepository, EventRepositoryExt};
pub use serialized_event::{SerializedEvent, deserialize_events, serialize_events};
pub use serialized_snapshot::SerializedSnapshot;
pub use snapshot_repository::{SnapshotPolicy, SnapshotRepository, SnapshotRepositoryWithPolicy};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aggregate::Aggregate;
    use crate::domain_event::{BusinessContext, EventEnvelope};
    use crate::entiry::Entity;
    use crate::error::DomainError;
    use crate::event_upcaster::{EventUpcaster, EventUpcasterChain, EventUpcasterResult};
    use chrono::Utc;
    use ddd_macros::{entity, event};
    use serde::{Deserialize, Serialize};
    use std::sync::Arc;

    #[entity]
    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    struct User {
        name: String,
    }

    #[event(version = 2)]
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    enum UserEvent {
        Created { name: String },
    }

    impl Aggregate for User {
        const TYPE: &'static str = "user";
        type Command = ();
        type Event = UserEvent;
        type Error = DomainError;
        fn execute(&self, _command: Self::Command) -> Result<Vec<Self::Event>, Self::Error> {
            Ok(vec![])
        }
        fn apply(&mut self, event: &Self::Event) {
            match event {
                UserEvent::Created {
                    aggregate_version,
                    name,
                    ..
                } => {
                    self.name = name.clone();
                    self.version = *aggregate_version;
                }
            }
        }
    }

    #[test]
    fn serialize_deserialize_roundtrip() {
        let env = EventEnvelope::<User>::new(
            &"u-1".to_string(),
            UserEvent::Created {
                id: ulid::Ulid::new().to_string(),
                aggregate_version: 1,
                name: "alice".into(),
            },
            BusinessContext::builder()
                .maybe_correlation_id(Some("c-1".into()))
                .maybe_causation_id(Some("cause-1".into()))
                .maybe_actor_type(Some("user".into()))
                .maybe_actor_id(Some("u-actor".into()))
                .build(),
        );

        let ser = serialize_events(&[env.clone()]).unwrap();
        assert_eq!(ser.len(), 1);
        assert_eq!(ser[0].aggregate_id(), "u-1");
        assert_eq!(ser[0].aggregate_type(), User::TYPE);
        assert_eq!(ser[0].aggregate_version(), 1);
        assert_eq!(ser[0].correlation_id(), Some("c-1"));
        assert_eq!(ser[0].actor_type(), Some("user"));
        assert_eq!(ser[0].actor_id(), Some("u-actor"));

        let chain = EventUpcasterChain::default();
        let de = deserialize_events::<User>(&chain, ser).unwrap();
        assert_eq!(de.len(), 1);
        assert_eq!(de[0].payload, env.payload);
        assert_eq!(de[0].metadata.aggregate_id(), env.metadata.aggregate_id());
    }

    // Upcaster：将旧版本的 Created { username } 升级为 v2 的 Created { name }
    struct CreatedV1ToV2;
    impl EventUpcaster for CreatedV1ToV2 {
        fn applies(&self, event_type: &str, event_version: usize) -> bool {
            event_type == "UserEvent.Created" && event_version == 1
        }
        fn upcast(
            &self,
            event: SerializedEvent,
        ) -> crate::error::DomainResult<EventUpcasterResult> {
            let mut p = event.payload().clone();
            // 形状：{"Created": { id, aggregate_version, username }}
            if let Some(obj) = p.as_object_mut() {
                if let Some(inner) = obj.get_mut("Created").and_then(|v| v.as_object_mut()) {
                    if let Some(u) = inner.remove("username") {
                        inner.insert("name".to_string(), u);
                    }
                }
            }
            Ok(EventUpcasterResult::One(
                SerializedEvent::builder()
                    .event_id(event.event_id().to_string())
                    .event_type("UserEvent.Created".to_string())
                    .event_version(2)
                    .maybe_sequence_number(None)
                    .aggregate_id(event.aggregate_id().to_string())
                    .aggregate_type(event.aggregate_type().to_string())
                    .aggregate_version(event.aggregate_version())
                    .maybe_correlation_id(event.correlation_id().map(|s| s.to_string()))
                    .maybe_causation_id(event.causation_id().map(|s| s.to_string()))
                    .maybe_actor_type(event.actor_type().map(|s| s.to_string()))
                    .maybe_actor_id(event.actor_id().map(|s| s.to_string()))
                    .occurred_at(event.occurred_at())
                    .payload(p)
                    .context(serde_json::json!({
                        "upcasted": true,
                        "from_version": 1,
                        "upcaster": "CreatedV1ToV2",
                        "field_renamed": "username -> name"
                    }))
                    .build(),
            ))
        }
    }

    #[test]
    fn deserialize_with_upcast_compat_legacy_payload() {
        let payload = serde_json::json!({
            "Created": { "id": ulid::Ulid::new().to_string(), "aggregate_version": 1, "username": "alice" }
        });
        let raw = SerializedEvent::builder()
            .event_id(ulid::Ulid::new().to_string())
            .event_type("UserEvent.Created".to_string())
            .event_version(1)
            .maybe_sequence_number(None)
            .aggregate_id("u-2".to_string())
            .aggregate_type("user".to_string())
            .aggregate_version(1)
            .maybe_correlation_id(Some("c-legacy".into()))
            .maybe_causation_id(Some("cause-legacy".into()))
            .maybe_actor_type(Some("user".into()))
            .maybe_actor_id(Some("u-actor".into()))
            .occurred_at(Utc::now())
            .payload(payload)
            .context(
                serde_json::to_value(
                    &BusinessContext::builder()
                        .maybe_correlation_id(Some("c-legacy".into()))
                        .maybe_causation_id(Some("cause-legacy".into()))
                        .maybe_actor_type(Some("user".into()))
                        .maybe_actor_id(Some("u-actor".into()))
                        .build(),
                )
                .expect("serialize BusinessContext"),
            )
            .build();

        let chain: EventUpcasterChain = vec![Arc::new(CreatedV1ToV2) as Arc<dyn EventUpcaster>]
            .into_iter()
            .collect();
        let out = deserialize_events::<User>(&chain, vec![raw]).unwrap();
        assert_eq!(out.len(), 1);
        match &out[0].payload {
            UserEvent::Created { name, .. } => assert_eq!(name, "alice"),
        }
    }

    #[test]
    fn snapshot_serde_and_type_check() {
        let u = <User as Entity>::new("u-1".to_string());
        let snap = SerializedSnapshot::from_aggregate(&u).unwrap();
        assert_eq!(snap.aggregate_id(), "u-1");
        assert_eq!(snap.aggregate_type(), User::TYPE);
        assert_eq!(snap.aggregate_version(), 0);

        let restored: User = snap.to_aggregate().unwrap();
        assert_eq!(restored.id(), u.id());

        // 类型不匹配应报错
        #[entity]
        #[derive(Debug, Clone, Default, Serialize, Deserialize)]
        struct Order {}
        impl Aggregate for Order {
            const TYPE: &'static str = "order";
            type Command = ();
            type Event = UserEvent;
            type Error = DomainError;
            fn execute(&self, _c: Self::Command) -> Result<Vec<Self::Event>, Self::Error> {
                Ok(vec![])
            }
            fn apply(&mut self, _e: &Self::Event) {}
        }

        let err = snap.to_aggregate::<Order>().unwrap_err();
        match err {
            DomainError::TypeMismatch { .. } => {}
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn snapshot_policy_should_snapshot() {
        assert!(!SnapshotPolicy::Never.should_snapshot(1));
        for v in 1..=9 {
            let should = SnapshotPolicy::Every(3).should_snapshot(v);
            assert_eq!(should, v % 3 == 0);
        }
    }
}
