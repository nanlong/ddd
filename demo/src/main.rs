use async_trait::async_trait;
use ddd::aggregate::Aggregate;
use ddd::aggregate_root::AggregateRoot;
use ddd::domain_event::{AggregateEvents, DomainEvent, EventEnvelope, Metadata};
use ddd::repository::Repository;
use ddd_macros::{aggregate, event};
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::sync::{Arc, Mutex};
use ulid::Ulid;

#[aggregate(id = String)]
#[derive(Debug, Clone)]
struct Account {
    balance: i64,
}

#[derive(Debug)]
enum AccountError {
    AlreadyOpened,
    NotOpened,
    InsufficientFunds,
}

impl Display for AccountError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyOpened => write!(f, "account already opened"),
            Self::NotOpened => write!(f, "account not opened"),
            Self::InsufficientFunds => write!(f, "insufficient funds"),
        }
    }
}
impl std::error::Error for AccountError {}

#[derive(Debug)]
enum AccountCommand {
    Open { initial_balance: i64 },
    Deposit { amount: i64 },
    Withdraw { amount: i64 },
}

#[event]
#[derive(Debug, Clone)]
enum AccountEvent {
    Opened { initial_balance: i64 },
    Deposited { amount: i64 },
    Withdrawn { amount: i64 },
}

impl DomainEvent for AccountEvent {
    fn event_type(&self) -> String {
        match self {
            AccountEvent::Opened { .. } => "account.opened",
            AccountEvent::Deposited { .. } => "account.deposited",
            AccountEvent::Withdrawn { .. } => "account.withdrawn",
        }
        .to_string()
    }

    fn event_version(&self) -> i64 {
        match self {
            AccountEvent::Opened { version, .. }
            | AccountEvent::Deposited { version, .. }
            | AccountEvent::Withdrawn { version, .. } => *version,
        }
    }
}

#[async_trait]
impl Aggregate for Account {
    const TYPE: &'static str = "account";

    type Id = String;
    type Command = AccountCommand;
    type Event = AccountEvent;
    type Error = AccountError;

    fn new(aggregate_id: &str) -> Self {
        Self {
            id: aggregate_id.to_string(),
            version: 0,
            balance: 0,
        }
    }

    fn id(&self) -> &Self::Id {
        &self.id
    }

    fn version(&self) -> i64 {
        self.version
    }

    async fn execute(&self, command: Self::Command) -> Result<Vec<Self::Event>, Self::Error> {
        match command {
            AccountCommand::Open { initial_balance } => {
                if self.version > 0 {
                    return Err(AccountError::AlreadyOpened);
                }
                let evt = AccountEvent::Opened {
                    id: Ulid::new().to_string(),
                    version: self.version + 1,
                    initial_balance,
                };
                Ok(vec![evt])
            }
            AccountCommand::Deposit { amount } => {
                if self.version == 0 {
                    return Err(AccountError::NotOpened);
                }
                let evt = AccountEvent::Deposited {
                    id: Ulid::new().to_string(),
                    version: self.version + 1,
                    amount,
                };
                Ok(vec![evt])
            }
            AccountCommand::Withdraw { amount } => {
                if self.version == 0 {
                    return Err(AccountError::NotOpened);
                }
                if self.balance < amount {
                    return Err(AccountError::InsufficientFunds);
                }
                let evt = AccountEvent::Withdrawn {
                    id: Ulid::new().to_string(),
                    version: self.version + 1,
                    amount,
                };
                Ok(vec![evt])
            }
        }
    }

    fn apply(&mut self, event: &Self::Event) {
        match event {
            AccountEvent::Opened {
                version,
                initial_balance,
                ..
            } => {
                self.balance = *initial_balance;
                self.version = *version;
            }
            AccountEvent::Deposited {
                version, amount, ..
            } => {
                self.balance += *amount;
                self.version = *version;
            }
            AccountEvent::Withdrawn {
                version, amount, ..
            } => {
                self.balance -= *amount;
                self.version = *version;
            }
        }
    }
}

#[derive(Default, Clone)]
struct InMemoryAccountRepo {
    inner: Arc<Mutex<HashMap<String, Vec<EventEnvelope<Account>>>>>,
}

#[async_trait]
impl Repository<Account> for InMemoryAccountRepo {
    async fn load_events(
        &self,
        aggregate_id: &str,
    ) -> Result<AggregateEvents<Account>, AccountError> {
        let store = self.inner.lock().unwrap();
        let events = store
            .get(aggregate_id)
            .cloned()
            .unwrap_or_else(|| Vec::new());
        Ok(AggregateEvents::new(events))
    }

    async fn load_aggregate(&self, aggregate_id: &str) -> Result<Option<Account>, AccountError> {
        let store = self.inner.lock().unwrap();
        if let Some(events) = store.get(aggregate_id) {
            let mut acc = Account::new(aggregate_id);
            for env in events.iter() {
                acc.apply(&env.event);
            }
            Ok(Some(acc))
        } else {
            Ok(None)
        }
    }

    async fn commit(
        &self,
        aggregate: &Account,
        events: Vec<AccountEvent>,
        metadata: Metadata,
    ) -> Result<Vec<EventEnvelope<Account>>, AccountError> {
        let mut store = self.inner.lock().unwrap();
        let entry = store.entry(aggregate.id().to_string()).or_default();
        let mut out = Vec::with_capacity(events.len());
        for e in events {
            let eid = ulid::Ulid::new().to_string();
            let env = EventEnvelope::<Account>::new(eid, aggregate, e, metadata.clone());
            entry.push(env.clone());
            out.push(env);
        }
        Ok(out)
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let repo = InMemoryAccountRepo::default();
    let root = AggregateRoot::<Account, _>::new(repo.clone());
    let id = "acc-1";

    // 开户
    let events = root
        .execute(
            id,
            AccountCommand::Open {
                initial_balance: 1000,
            },
            Metadata::default(),
        )
        .await
        .unwrap();
    println!("opened: {} events", events.len());
    println!("events: {:?}", events);

    // 存款
    let events = root
        .execute(
            id,
            AccountCommand::Deposit { amount: 500 },
            Metadata::default(),
        )
        .await
        .unwrap();
    println!("deposited: {} events", events.len());
    println!("events: {:?}", events);

    // 取款
    let events = root
        .execute(
            id,
            AccountCommand::Withdraw { amount: 200 },
            Metadata::default(),
        )
        .await
        .unwrap();
    println!("withdrawn: {} events", events.len());
    println!("events: {:?}", events);

    // 重新加载并打印状态
    let loaded = repo.load_aggregate(id).await.unwrap().unwrap();
    println!(
        "reloaded: id={}, balance={}, version= {}",
        loaded.id(),
        loaded.balance,
        loaded.version()
    );
}
