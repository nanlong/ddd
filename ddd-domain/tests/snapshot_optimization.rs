use anyhow::Result as AnyResult;
use async_trait::async_trait;
use chrono::Utc;
use ddd_domain::aggregate::Aggregate;
use ddd_domain::domain_event::EventContext;
use ddd_domain::entity::Entity;
use ddd_domain::error::{DomainError, DomainResult};
use ddd_domain::persist::{
    AggregateRepository, EventRepository, SerializedEvent, SerializedSnapshot, SnapshotPolicy,
    SnapshotPolicyRepo, SnapshotRepository, SnapshotRepositoryWithPolicy,
};
use ddd_macros::{domain_event, entity};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[entity]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Counter {
    value: i64,
}

#[domain_event(version = 1)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum CounterEvent {
    Incr { by: i64 },
}

impl Aggregate for Counter {
    const TYPE: &'static str = "counter";
    type Command = ();
    type Event = CounterEvent;
    type Error = DomainError;
    fn execute(&self, _c: Self::Command) -> Result<Vec<Self::Event>, Self::Error> {
        Ok(vec![])
    }
    fn apply(&mut self, e: &Self::Event) {
        match e {
            CounterEvent::Incr {
                aggregate_version,
                by,
                ..
            } => {
                self.value += *by;
                self.version = *aggregate_version;
            }
        }
    }
}

#[derive(Default, Clone)]
struct CountingEventRepo {
    events: Arc<Mutex<HashMap<String, Vec<SerializedEvent>>>>,
    pub get_all_calls: Arc<Mutex<usize>>,
    pub get_last_calls: Arc<Mutex<usize>>,
}

#[async_trait]
impl EventRepository for CountingEventRepo {
    async fn get_events<A: Aggregate>(
        &self,
        aggregate_id: &A::Id,
    ) -> DomainResult<Vec<SerializedEvent>> {
        *self.get_all_calls.lock().unwrap() += 1;
        Ok(self
            .events
            .lock()
            .unwrap()
            .get(&aggregate_id.to_string())
            .cloned()
            .unwrap_or_default())
    }
    async fn get_last_events<A: Aggregate>(
        &self,
        aggregate_id: &A::Id,
        last_version: usize,
    ) -> DomainResult<Vec<SerializedEvent>> {
        *self.get_last_calls.lock().unwrap() += 1;
        Ok(self
            .events
            .lock()
            .unwrap()
            .get(&aggregate_id.to_string())
            .map(|v| {
                v.iter()
                    .filter(|e| e.aggregate_version() > last_version)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default())
    }
    async fn save(&self, events: Vec<SerializedEvent>) -> DomainResult<()> {
        if events.is_empty() {
            return Ok(());
        }
        let mut g = self.events.lock().unwrap();
        let k = events[0].aggregate_id().to_string();
        g.entry(k).or_default().extend_from_slice(&events);
        Ok(())
    }
}

#[derive(Default, Clone)]
struct InMemorySnapshotPolicyRepo {
    snaps: Arc<Mutex<HashMap<String, SerializedSnapshot>>>,
}

#[async_trait]
impl SnapshotRepository for InMemorySnapshotPolicyRepo {
    async fn get_snapshot<A: Aggregate>(
        &self,
        aggregate_id: &A::Id,
        _version: Option<usize>,
    ) -> DomainResult<Option<SerializedSnapshot>> {
        Ok(self
            .snaps
            .lock()
            .unwrap()
            .get(&aggregate_id.to_string())
            .cloned())
    }
    async fn save<A: Aggregate>(&self, aggregate: &A) -> DomainResult<()> {
        let snap = SerializedSnapshot::from_aggregate(aggregate)?;
        self.snaps
            .lock()
            .unwrap()
            .insert(aggregate.id().to_string(), snap);
        Ok(())
    }
}

fn mk_incr(id: &str, version: usize, by: i64) -> SerializedEvent {
    let eid = ulid::Ulid::new().to_string();
    let payload = serde_json::json!({"Incr": {"id": eid, "aggregate_version": version, "by": by }});
    let event_context = EventContext::builder()
        .maybe_correlation_id(Some(format!("cor-{id}")))
        .maybe_causation_id(Some(format!("cau-{id}")))
        .maybe_actor_type(Some("user".into()))
        .maybe_actor_id(Some("u-1".into()))
        .build();

    SerializedEvent::builder()
        .event_id(eid)
        .event_type("CounterEvent.Incr".into())
        .event_version(1)
        .maybe_sequence_number(None)
        .aggregate_id(id.to_string())
        .aggregate_type("counter".into())
        .aggregate_version(version)
        .correlation_id(format!("cor-{id}"))
        .causation_id(format!("cau-{id}"))
        .actor_type("user".into())
        .actor_id("u-1".into())
        .occurred_at(Utc::now())
        .payload(payload)
        .context(serde_json::to_value(&event_context).expect("serialize EventContext"))
        .build()
}

#[tokio::test]
async fn snapshot_optimization_by_call_count() -> AnyResult<()> {
    let repo = Arc::new(CountingEventRepo::default());
    let snaps = Arc::new(SnapshotRepositoryWithPolicy::new(
        InMemorySnapshotPolicyRepo::default(),
        SnapshotPolicy::Every(1),
    ));
    let chain = Arc::new(ddd_domain::event_upcaster::EventUpcasterChain::default());
    let store = SnapshotPolicyRepo::new(repo.clone(), snaps.clone(), chain);

    let id = "c-1".to_string();

    // 写入大量历史事件（版本 1..=100）
    let mut all = Vec::new();
    for v in 1..=100 {
        all.push(mk_incr(&id, v, 1));
    }
    repo.save(all).await?;

    // 保存快照（版本 100）
    let mut agg = <Counter as Entity>::new(id.clone(), 0);
    for v in 1..=100 {
        agg.apply(&CounterEvent::Incr {
            id: ulid::Ulid::new().to_string(),
            aggregate_version: v,
            by: 1,
        });
    }
    snaps.save(&agg).await?;

    // 追加增量事件（101..105）
    let mut inc = Vec::new();
    for v in 101..=105 {
        inc.push(mk_incr(&id, v, 1));
    }
    repo.save(inc).await?;

    // 加载（应当仅调用一次 get_last_events，且不调用 get_events）
    let loaded: Counter = store.load(&id).await?.unwrap();
    assert_eq!(loaded.version(), 105);
    assert_eq!(loaded.value, 105);
    assert_eq!(*repo.get_all_calls.lock().unwrap(), 0);
    assert_eq!(*repo.get_last_calls.lock().unwrap(), 1);
    Ok(())
}
