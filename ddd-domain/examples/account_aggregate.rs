/// Account 聚合示例
/// 演示基于命令驱动的事件溯源：打开账户、存取款等
use async_trait::async_trait;
use ddd_domain::aggregate::Aggregate;
use ddd_domain::aggregate_root::AggregateRoot;
use ddd_domain::domain_event::{BusinessContext, EventEnvelope};
use ddd_domain::entiry::Entity;
use ddd_domain::error::DomainError;
use ddd_domain::persist::AggregateRepository;
use ddd_macros::{entity, entity_id, event};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use ulid::Ulid;

// ============================================================================
// 领域模型定义
// ============================================================================

#[entity_id]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct AccountId(Ulid);

#[entity(id = AccountId)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Account {
    balance: usize,
}

#[derive(Debug)]
enum AccountCommand {
    Open { initial_balance: usize },
    Deposit { amount: usize },
    Withdraw { amount: usize },
}

#[event(version = 1)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum AccountEvent {
    #[event(event_type = "account.opened")]
    Opened { initial_balance: usize },
    #[event(event_type = "account.deposited")]
    Deposited { amount: usize },
    #[event(event_type = "account.withdrawn")]
    Withdrawn { amount: usize },
}

impl Aggregate for Account {
    const TYPE: &'static str = "account";
    type Command = AccountCommand;
    type Event = AccountEvent;
    type Error = DomainError;

    fn execute(&self, command: Self::Command) -> Result<Vec<Self::Event>, Self::Error> {
        match command {
            AccountCommand::Open { initial_balance } => {
                if self.version() > 0 {
                    return Err(DomainError::InvalidState {
                        reason: "account already opened".to_string(),
                    });
                }
                let evt = AccountEvent::Opened {
                    id: Ulid::new().to_string(),
                    aggregate_version: self.version() + 1,
                    initial_balance,
                };
                Ok(vec![evt])
            }
            AccountCommand::Deposit { amount } => {
                if self.version() == 0 {
                    return Err(DomainError::InvalidState {
                        reason: "account not opened".to_string(),
                    });
                }
                let evt = AccountEvent::Deposited {
                    id: Ulid::new().to_string(),
                    aggregate_version: self.version() + 1,
                    amount,
                };
                Ok(vec![evt])
            }
            AccountCommand::Withdraw { amount } => {
                if self.version() == 0 {
                    return Err(DomainError::InvalidState {
                        reason: "account not opened".to_string(),
                    });
                }
                if self.balance < amount {
                    return Err(DomainError::InvalidState {
                        reason: "insufficient funds".to_string(),
                    });
                }
                let evt = AccountEvent::Withdrawn {
                    id: Ulid::new().to_string(),
                    aggregate_version: self.version() + 1,
                    amount,
                };
                Ok(vec![evt])
            }
        }
    }

    fn apply(&mut self, event: &Self::Event) {
        match event {
            AccountEvent::Opened {
                aggregate_version,
                initial_balance,
                ..
            } => {
                self.balance = *initial_balance;
                self.version = *aggregate_version;
            }
            AccountEvent::Deposited {
                aggregate_version,
                amount,
                ..
            } => {
                self.balance += *amount;
                self.version = *aggregate_version;
            }
            AccountEvent::Withdrawn {
                aggregate_version,
                amount,
                ..
            } => {
                self.balance -= *amount;
                self.version = *aggregate_version;
            }
        }
    }
}

#[derive(Default, Clone)]
struct InMemoryAccountRepo {
    inner: Arc<Mutex<HashMap<String, Vec<EventEnvelope<Account>>>>>,
}

#[async_trait]
impl AggregateRepository<Account> for InMemoryAccountRepo {
    async fn load(&self, aggregate_id: &str) -> Result<Option<Account>, DomainError> {
        let store = self.inner.lock().unwrap();
        if let Some(events) = store.get(aggregate_id) {
            let mut acc = <Account as Entity>::new(AccountId::from_str(aggregate_id).unwrap());
            for env in events.iter() {
                acc.apply(&env.payload);
            }
            Ok(Some(acc))
        } else {
            Ok(None)
        }
    }

    async fn save(
        &self,
        aggregate: &Account,
        events: Vec<AccountEvent>,
        context: BusinessContext,
    ) -> Result<Vec<EventEnvelope<Account>>, DomainError> {
        let mut store = self.inner.lock().unwrap();
        let entry = store.entry(aggregate.id().to_string()).or_default();
        let mut out = Vec::with_capacity(events.len());
        for e in events {
            let env = EventEnvelope::<Account>::new(aggregate.id(), e, context.clone());
            entry.push(env.clone());
            out.push(env);
        }
        Ok(out)
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    println!("=== Account 聚合示例 ===\n");
    let repo = InMemoryAccountRepo::default();
    let root = AggregateRoot::<Account, _>::new(repo.clone());
    let id = AccountId::from_str("acc-1").unwrap();

    // 开户
    let events = root
        .execute(
            &id,
            AccountCommand::Open {
                initial_balance: 1000,
            },
            BusinessContext::default(),
        )
        .await
        .unwrap();
    println!("✅ 开户，产生 {} 个事件", events.len());
    println!("events: {:?}", events);

    // 存款
    let events = root
        .execute(
            &id,
            AccountCommand::Deposit { amount: 500 },
            BusinessContext::default(),
        )
        .await
        .unwrap();
    println!("✅ 存款 +500，产生 {} 个事件", events.len());
    println!("events: {:?}", events);

    // 取款
    let events = root
        .execute(
            &id,
            AccountCommand::Withdraw { amount: 200 },
            BusinessContext::default(),
        )
        .await
        .unwrap();
    println!("✅ 取款 -200，产生 {} 个事件", events.len());
    println!("events: {:?}", events);

    // 重新加载并打印状态
    let loaded = repo.load(&id.to_string()).await.unwrap().unwrap();
    println!("\n--- 重新加载聚合 ---");
    println!(
        "聚合: id={}, 版本={}, 余额={}",
        loaded.id(),
        loaded.version(),
        loaded.balance
    );
}
