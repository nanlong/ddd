use async_trait::async_trait;
use anyhow::Result as AnyResult;
use ddd::aggregate::Aggregate;
use ddd::aggregate_root::AggregateRoot;
use ddd::domain_event::{BusinessContext, EventEnvelope};
use ddd::error::{DomainError, DomainResult};
use ddd::persist::{
    AggregateRepository, EventRepository, EventStoreAggregateRepository, SerializedEvent,
    serialize_events,
};
use ddd_macros::{aggregate, event};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[aggregate]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct BankAccount {
    balance: i64,
    is_locked: bool,
}

#[derive(Debug)]
enum Cmd {
    Deposit { amount: i64 },
    Withdraw { amount: i64 },
}

#[event(version = 1)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum Evt {
    Deposited { amount: i64 },
    Withdrawn { amount: i64 },
    Locked { reason: String },
    Unlocked { reason: String },
}

impl Aggregate for BankAccount {
    const TYPE: &'static str = "bank_account";
    type Id = String;
    type Command = Cmd;
    type Event = Evt;
    type Error = DomainError;
    fn new(aggregate_id: Self::Id) -> Self {
        Self {
            id: aggregate_id,
            version: 0,
            balance: 0,
            is_locked: false,
        }
    }
    fn id(&self) -> &Self::Id {
        &self.id
    }
    fn version(&self) -> usize {
        self.version
    }
    fn execute(&self, command: Self::Command) -> Result<Vec<Self::Event>, Self::Error> {
        match command {
            Cmd::Deposit { amount } => {
                if amount <= 0 || self.is_locked {
                    return Err(DomainError::InvalidCommand {
                        reason: "bad".into(),
                    });
                }
                Ok(vec![Evt::Deposited {
                    id: ulid::Ulid::new().to_string(),
                    aggregate_version: self.version() + 1,
                    amount,
                }])
            }
            Cmd::Withdraw { amount } => {
                if amount <= 0 || self.is_locked || self.balance < amount {
                    return Err(DomainError::InvalidState {
                        reason: "bad".into(),
                    });
                }
                Ok(vec![Evt::Withdrawn {
                    id: ulid::Ulid::new().to_string(),
                    aggregate_version: self.version() + 1,
                    amount,
                }])
            }
        }
    }
    fn apply(&mut self, e: &Self::Event) {
        match e {
            Evt::Deposited {
                aggregate_version,
                amount,
                ..
            } => {
                self.balance += amount;
                self.version = *aggregate_version;
            }
            Evt::Withdrawn {
                aggregate_version,
                amount,
                ..
            } => {
                self.balance -= amount;
                self.version = *aggregate_version;
            }
            Evt::Locked {
                aggregate_version, ..
            } => {
                self.is_locked = true;
                self.version = *aggregate_version;
            }
            Evt::Unlocked {
                aggregate_version, ..
            } => {
                self.is_locked = false;
                self.version = *aggregate_version;
            }
        }
    }
}

#[derive(Default, Clone)]
struct InMemoryEventRepository {
    inner: Arc<Mutex<HashMap<String, Vec<SerializedEvent>>>>,
}

#[async_trait]
impl EventRepository for InMemoryEventRepository {
    async fn get_events<A: Aggregate>(
        &self,
        aggregate_id: &str,
    ) -> DomainResult<Vec<SerializedEvent>> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .get(aggregate_id)
            .cloned()
            .unwrap_or_default())
    }
    async fn get_last_events<A: Aggregate>(
        &self,
        aggregate_id: &str,
        last_version: usize,
    ) -> DomainResult<Vec<SerializedEvent>> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .get(aggregate_id)
            .map(|v| {
                v.iter()
                    .cloned()
                    .filter(|e| e.aggregate_version() > last_version)
                    .collect()
            })
            .unwrap_or_default())
    }
    async fn save(&self, events: &[SerializedEvent]) -> DomainResult<()> {
        if events.is_empty() {
            return Ok(());
        }
        let mut m = self.inner.lock().unwrap();
        let key = events[0].aggregate_id().to_string();
        m.entry(key).or_default().extend_from_slice(events);
        Ok(())
    }
}

#[tokio::test]
async fn aggregate_persist_and_load_flow() -> AnyResult<()> {
    let event_repo = Arc::new(InMemoryEventRepository::default());
    let upcasters = Arc::new(ddd::event_upcaster::EventUpcasterChain::default());
    let repo = Arc::new(EventStoreAggregateRepository::<BankAccount, _>::new(
        event_repo.clone(),
        upcasters,
    ));
    let root = AggregateRoot::<BankAccount, _>::new(repo.clone());
    let id = "acc-1".to_string();

    // 执行命令 -> 事件 -> 持久化
    root.execute(
        &id,
        Cmd::Deposit { amount: 1000 },
        BusinessContext::default(),
    )
    .await?;
    root.execute(
        &id,
        Cmd::Withdraw { amount: 300 },
        BusinessContext::default(),
    )
    .await?;

    // 直接检查底层事件仓储
    let stored = event_repo.get_events::<BankAccount>(&id).await?;
    assert_eq!(stored.len(), 2);
    assert_eq!(stored[0].aggregate_version(), 1);
    assert_eq!(stored[1].aggregate_version(), 2);

    // 使用仓储加载聚合
    let loaded = repo.load(&id).await?.unwrap();
    assert_eq!(loaded.balance, 700);
    assert_eq!(loaded.version(), 2);

    // 追加锁定/解锁，确保状态可往返
    let evs = vec![Evt::Locked {
        id: ulid::Ulid::new().to_string(),
        aggregate_version: 3,
        reason: "m".into(),
    }];
    let envs: Vec<EventEnvelope<BankAccount>> = evs
        .into_iter()
        .map(|e| EventEnvelope::new(&id, e, BusinessContext::default()))
        .collect();
    let ser = serialize_events(&envs).unwrap();
    event_repo.save(&ser).await?;
    let loaded2 = repo.load(&id).await?.unwrap();
    assert!(loaded2.is_locked);
    assert_eq!(loaded2.version(), 3);
    Ok(())
}
