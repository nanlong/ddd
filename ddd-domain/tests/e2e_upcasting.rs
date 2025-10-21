use anyhow::Result as AnyResult;
use async_trait::async_trait;
use chrono::Utc;
use ddd_domain::aggregate::Aggregate;
use ddd_domain::domain_event::BusinessContext;
use ddd_domain::entity::Entity;
use ddd_domain::error::{DomainError, DomainResult};
use ddd_domain::event_upcaster::{EventUpcaster, EventUpcasterChain, EventUpcasterResult};
use ddd_domain::persist::{
    AggregateRepository, EventRepository, EventStoreAggregateRepository, SerializedEvent,
};
use ddd_macros::{entity, event};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[entity]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Wallet {
    balance_minor_units: i64,
    currency: String,
}

#[event(version = 3)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum WalletEvent {
    Deposited { minor_units: i64, currency: String },
}

impl Aggregate for Wallet {
    const TYPE: &'static str = "wallet";
    type Command = ();
    type Event = WalletEvent;
    type Error = DomainError;
    fn execute(&self, _c: Self::Command) -> Result<Vec<Self::Event>, Self::Error> {
        Ok(vec![])
    }
    fn apply(&mut self, e: &Self::Event) {
        match e {
            WalletEvent::Deposited {
                aggregate_version,
                minor_units,
                currency,
                ..
            } => {
                self.currency = currency.clone();
                self.balance_minor_units += *minor_units;
                self.version = if *aggregate_version == 0 {
                    self.version + 1
                } else {
                    *aggregate_version
                };
            }
        }
    }
}

// v1 -> v2: add currency field, rename amount -> amount
struct V1ToV2;
impl EventUpcaster for V1ToV2 {
    fn applies(&self, event_type: &str, event_version: usize) -> bool {
        event_type == "WalletEvent.Deposited" && event_version == 1
    }
    fn upcast(&self, event: SerializedEvent) -> DomainResult<EventUpcasterResult> {
        let mut p = event.payload().clone();
        if let Some(obj) = p.as_object_mut()
            && let Some(inner) = obj.get_mut("Deposited").and_then(|v| v.as_object_mut())
        {
            inner
                .entry("currency".to_string())
                .or_insert(serde_json::json!("CNY"));
        }
        // 重建 BusinessContext 以保留原始事件的业务上下文
        let business_context = BusinessContext::builder()
            .maybe_correlation_id(event.correlation_id().map(|s| s.to_string()))
            .maybe_causation_id(event.causation_id().map(|s| s.to_string()))
            .maybe_actor_type(event.actor_type().map(|s| s.to_string()))
            .maybe_actor_id(event.actor_id().map(|s| s.to_string()))
            .build();

        Ok(EventUpcasterResult::One(
            SerializedEvent::builder()
                .event_id(event.event_id().to_string())
                .event_type(event.event_type().to_string())
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
                .context(serde_json::to_value(&business_context)?)
                .build(),
        ))
    }
}

// v2 -> v3: amount (yuan) -> minor_units (cents)
struct V2ToV3;
impl EventUpcaster for V2ToV3 {
    fn applies(&self, event_type: &str, event_version: usize) -> bool {
        event_type == "WalletEvent.Deposited" && event_version == 2
    }
    fn upcast(&self, event: SerializedEvent) -> DomainResult<EventUpcasterResult> {
        let mut p = event.payload().clone();
        if let Some(obj) = p.as_object_mut()
            && let Some(inner) = obj.get_mut("Deposited").and_then(|v| v.as_object_mut())
            && let Some(amount) = inner.remove("amount").and_then(|v| v.as_i64())
        {
            inner.insert("minor_units".to_string(), serde_json::json!(amount * 100));
        }
        // 重建 BusinessContext 以保留原始事件的业务上下文
        let business_context = BusinessContext::builder()
            .maybe_correlation_id(event.correlation_id().map(|s| s.to_string()))
            .maybe_causation_id(event.causation_id().map(|s| s.to_string()))
            .maybe_actor_type(event.actor_type().map(|s| s.to_string()))
            .maybe_actor_id(event.actor_id().map(|s| s.to_string()))
            .build();

        Ok(EventUpcasterResult::One(
            SerializedEvent::builder()
                .event_id(event.event_id().to_string())
                .event_type(event.event_type().to_string())
                .event_version(3)
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
                .context(serde_json::to_value(&business_context)?)
                .build(),
        ))
    }
}

#[derive(Default, Clone)]
struct MemRepo {
    m: Arc<Mutex<HashMap<String, Vec<SerializedEvent>>>>,
}
#[async_trait]
impl EventRepository for MemRepo {
    async fn get_events<A: Aggregate>(&self, id: &str) -> DomainResult<Vec<SerializedEvent>> {
        Ok(self.m.lock().unwrap().get(id).cloned().unwrap_or_default())
    }
    async fn get_last_events<A: Aggregate>(
        &self,
        id: &str,
        last: usize,
    ) -> DomainResult<Vec<SerializedEvent>> {
        Ok(self
            .m
            .lock()
            .unwrap()
            .get(id)
            .map(|v| {
                v.iter()
                    .filter(|e| e.aggregate_version() > last)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default())
    }
    async fn save(&self, events: Vec<SerializedEvent>) -> DomainResult<()> {
        if events.is_empty() {
            return Ok(());
        }
        let mut g = self.m.lock().unwrap();
        let k = events[0].aggregate_id().to_string();
        g.entry(k).or_default().extend_from_slice(&events);
        Ok(())
    }
}

fn mk_v1(id: &str, amount_yuan: i64) -> SerializedEvent {
    let eid = ulid::Ulid::new().to_string();
    let payload = serde_json::json!({"Deposited": {"id": eid, "aggregate_version": 0, "amount": amount_yuan }});
    let biz = BusinessContext::builder()
        .maybe_correlation_id(Some(format!("cor-{id}")))
        .maybe_causation_id(Some(format!("cau-{id}")))
        .maybe_actor_type(Some("user".into()))
        .maybe_actor_id(Some("u-1".into()))
        .build();

    SerializedEvent::builder()
        .event_id(eid)
        .event_type("WalletEvent.Deposited".into())
        .event_version(1)
        .maybe_sequence_number(None)
        .aggregate_id(id.to_string())
        .aggregate_type("wallet".into())
        .aggregate_version(0)
        .correlation_id(format!("cor-{id}"))
        .causation_id(format!("cau-{id}"))
        .actor_type("user".into())
        .actor_id("u-1".into())
        .occurred_at(Utc::now())
        .payload(payload)
        .context(serde_json::to_value(&biz).expect("serialize BusinessContext"))
        .build()
}

fn mk_v2(id: &str, amount_yuan: i64, currency: &str) -> SerializedEvent {
    let eid = ulid::Ulid::new().to_string();
    let payload = serde_json::json!({"Deposited": {"id": eid, "aggregate_version": 0, "amount": amount_yuan, "currency": currency }});
    let biz = BusinessContext::builder()
        .maybe_correlation_id(Some(format!("cor-{id}")))
        .maybe_causation_id(Some(format!("cau-{id}")))
        .maybe_actor_type(Some("user".into()))
        .maybe_actor_id(Some("u-1".into()))
        .build();

    SerializedEvent::builder()
        .event_id(eid)
        .event_type("WalletEvent.Deposited".into())
        .event_version(2)
        .maybe_sequence_number(None)
        .aggregate_id(id.to_string())
        .aggregate_type("wallet".into())
        .aggregate_version(0)
        .correlation_id(format!("cor-{id}"))
        .causation_id(format!("cau-{id}"))
        .actor_type("user".into())
        .actor_id("u-1".into())
        .occurred_at(Utc::now())
        .payload(payload)
        .context(serde_json::to_value(&biz).expect("serialize BusinessContext"))
        .build()
}

#[tokio::test]
async fn e2e_upcasting_end_to_end() -> AnyResult<()> {
    let repo = Arc::new(MemRepo::default());
    let upcasters: Arc<EventUpcasterChain> = Arc::new(
        vec![
            Arc::new(V1ToV2) as Arc<dyn EventUpcaster>,
            Arc::new(V2ToV3) as Arc<dyn EventUpcaster>,
        ]
        .into_iter()
        .collect(),
    );
    let store = Arc::new(EventStoreAggregateRepository::<Wallet, _>::new(
        repo.clone(),
        upcasters,
    ));

    let id = "w-1";
    let events = vec![
        mk_v1(id, 100),       // 100 元 -> 10000 分
        mk_v2(id, 30, "CNY"), // 30 元 -> 3000 分
    ];
    repo.save(events).await?;

    let agg = store.load(id).await?.unwrap();
    assert_eq!(agg.balance_minor_units, 13000);
    assert_eq!(agg.version(), 2);
    Ok(())
}
