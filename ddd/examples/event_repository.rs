/// EventRepository 示例
/// 演示如何实现事件仓储接口，用于持久化和查询领域事件
use anyhow::Result;
use async_trait::async_trait;
use ddd::aggregate::Aggregate;
use ddd::aggregate_root::AggregateRoot;
use ddd::domain_event::{BusinessContext, DomainEvent, EventEnvelope, Metadata};
use ddd::persist::{AggragateRepository, EventRepository};
use ddd_macros::{aggregate, event};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::sync::{Arc, Mutex};
use ulid::Ulid;

// ============================================================================
// 领域模型定义
// ============================================================================

#[aggregate]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct BankAccount {
    balance: i64,
    is_locked: bool,
}

#[derive(Debug)]
enum BankAccountError {
    InvalidId(String),
    AccountLocked,
    InsufficientBalance,
    NegativeAmount,
}

impl Display for BankAccountError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidId(msg) => write!(f, "invalid account id: {}", msg),
            Self::AccountLocked => write!(f, "account is locked"),
            Self::InsufficientBalance => write!(f, "insufficient balance"),
            Self::NegativeAmount => write!(f, "amount must be positive"),
        }
    }
}

impl std::error::Error for BankAccountError {}

impl From<std::string::ParseError> for BankAccountError {
    fn from(_: std::string::ParseError) -> Self {
        Self::InvalidId("parse error".to_string())
    }
}

impl From<anyhow::Error> for BankAccountError {
    fn from(err: anyhow::Error) -> Self {
        Self::InvalidId(err.to_string())
    }
}

#[derive(Debug)]
enum BankAccountCommand {
    Deposit { amount: i64 },
    Withdraw { amount: i64 },
    Lock,
    Unlock,
}

#[event]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum BankAccountEvent {
    Deposited { amount: i64 },
    Withdrawn { amount: i64 },
    Locked { reason: String },
    Unlocked { reason: String },
}

impl DomainEvent for BankAccountEvent {
    fn event_type(&self) -> String {
        match self {
            BankAccountEvent::Deposited { .. } => "bank_account.deposited",
            BankAccountEvent::Withdrawn { .. } => "bank_account.withdrawn",
            BankAccountEvent::Locked { .. } => "bank_account.locked",
            BankAccountEvent::Unlocked { .. } => "bank_account.unlocked",
        }
        .to_string()
    }

    fn event_version(&self) -> usize {
        match self {
            BankAccountEvent::Deposited { version, .. }
            | BankAccountEvent::Withdrawn { version, .. }
            | BankAccountEvent::Locked { version, .. }
            | BankAccountEvent::Unlocked { version, .. } => *version,
        }
    }
}

impl Aggregate for BankAccount {
    const TYPE: &'static str = "bank_account";

    type Id = String;
    type Command = BankAccountCommand;
    type Event = BankAccountEvent;
    type Error = BankAccountError;

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
            BankAccountCommand::Deposit { amount } => {
                if amount <= 0 {
                    return Err(BankAccountError::NegativeAmount);
                }
                if self.is_locked {
                    return Err(BankAccountError::AccountLocked);
                }
                Ok(vec![BankAccountEvent::Deposited {
                    id: Ulid::new().to_string(),
                    version: self.version() + 1,
                    amount,
                }])
            }
            BankAccountCommand::Withdraw { amount } => {
                if amount <= 0 {
                    return Err(BankAccountError::NegativeAmount);
                }
                if self.is_locked {
                    return Err(BankAccountError::AccountLocked);
                }
                if self.balance < amount {
                    return Err(BankAccountError::InsufficientBalance);
                }
                Ok(vec![BankAccountEvent::Withdrawn {
                    id: Ulid::new().to_string(),
                    version: self.version() + 1,
                    amount,
                }])
            }
            BankAccountCommand::Lock => {
                if self.is_locked {
                    return Ok(vec![]);
                }
                Ok(vec![BankAccountEvent::Locked {
                    id: Ulid::new().to_string(),
                    version: self.version() + 1,
                    reason: "Manual lock".to_string(),
                }])
            }
            BankAccountCommand::Unlock => {
                if !self.is_locked {
                    return Ok(vec![]);
                }
                Ok(vec![BankAccountEvent::Unlocked {
                    id: Ulid::new().to_string(),
                    version: self.version() + 1,
                    reason: "Manual unlock".to_string(),
                }])
            }
        }
    }

    fn apply(&mut self, event: &Self::Event) {
        match event {
            BankAccountEvent::Deposited {
                version, amount, ..
            } => {
                self.balance += amount;
                self.version = *version;
            }
            BankAccountEvent::Withdrawn {
                version, amount, ..
            } => {
                self.balance -= amount;
                self.version = *version;
            }
            BankAccountEvent::Locked { version, .. } => {
                self.is_locked = true;
                self.version = *version;
            }
            BankAccountEvent::Unlocked { version, .. } => {
                self.is_locked = false;
                self.version = *version;
            }
        }
    }
}

// ============================================================================
// 序列化事件定义
// ============================================================================

/// 序列化事件，用于持久化存储
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedEvent {
    pub aggregate_id: String,
    pub aggregate_type: String,
    pub event_type: String,
    pub event_version: usize,
    pub sequence: usize,
    pub metadata: Value,
    pub payload: Value,
    pub context: Value,
}

impl SerializedEvent {
    fn from_envelope<A: Aggregate>(envelope: &EventEnvelope<A>, sequence: usize) -> Result<Self> {
        let metadata_value = serde_json::to_value(&envelope.metadata)?;
        let metadata_map = metadata_value.as_object().unwrap();

        Ok(Self {
            aggregate_id: metadata_map["aggregate_id"].as_str().unwrap().to_string(),
            aggregate_type: metadata_map["aggregate_type"].as_str().unwrap().to_string(),
            event_type: envelope.payload.event_type(),
            event_version: envelope.payload.event_version(),
            sequence,
            metadata: metadata_value,
            payload: serde_json::to_value(&envelope.payload)?,
            context: serde_json::to_value(&envelope.context)?,
        })
    }

    fn to_envelope<A: Aggregate>(&self) -> Result<EventEnvelope<A>> {
        let metadata: Metadata = serde_json::from_value(self.metadata.clone())?;
        let payload: A::Event = serde_json::from_value(self.payload.clone())?;
        let context: BusinessContext = serde_json::from_value(self.context.clone())?;

        Ok(EventEnvelope {
            metadata,
            payload,
            context,
        })
    }
}

// ============================================================================
// 内存事件仓储实现
// ============================================================================

#[derive(Default, Clone)]
struct InMemoryEventRepository {
    // aggregate_id -> 事件列表
    events: Arc<Mutex<HashMap<String, Vec<SerializedEvent>>>>,
}

#[async_trait]
impl EventRepository for InMemoryEventRepository {
    type SerializedEvent = SerializedEvent;

    /// 获取聚合的所有事件
    async fn get_events<A: Aggregate>(
        &self,
        aggregate_id: &str,
    ) -> Result<Vec<Self::SerializedEvent>> {
        let events = self.events.lock().unwrap();
        Ok(events.get(aggregate_id).cloned().unwrap_or_else(Vec::new))
    }

    /// 获取聚合从指定版本之后的事件
    async fn get_last_events<A: Aggregate>(
        &self,
        aggregate_id: &str,
        last_version: usize,
    ) -> Result<Vec<Self::SerializedEvent>> {
        let events = self.events.lock().unwrap();
        Ok(events
            .get(aggregate_id)
            .map(|evts| {
                evts.iter()
                    .filter(|e| e.event_version > last_version)
                    .cloned()
                    .collect()
            })
            .unwrap_or_else(Vec::new))
    }

    /// 保存事件到仓储
    fn save<A: Aggregate>(&self, events: &[Self::SerializedEvent]) -> Result<()> {
        if events.is_empty() {
            return Ok(());
        }

        let mut store = self.events.lock().unwrap();
        let aggregate_id = &events[0].aggregate_id;

        let entry = store.entry(aggregate_id.clone()).or_default();
        entry.extend_from_slice(events);

        Ok(())
    }
}

// ============================================================================
// AggregateRepository 实现（整合 EventRepository）
// ============================================================================

#[derive(Clone)]
struct BankAccountRepository {
    event_repo: InMemoryEventRepository,
}

impl BankAccountRepository {
    fn new(event_repo: InMemoryEventRepository) -> Self {
        Self { event_repo }
    }
}

#[async_trait]
impl AggragateRepository<BankAccount> for BankAccountRepository {
    async fn load(&self, aggregate_id: &str) -> Result<Option<BankAccount>, BankAccountError> {
        let serialized = self
            .event_repo
            .get_events::<BankAccount>(aggregate_id)
            .await?;

        if serialized.is_empty() {
            return Ok(None);
        }

        let envelopes = deserialize_events::<BankAccount>(&serialized)?;
        let mut account = BankAccount::new(aggregate_id.to_string());
        for envelope in envelopes.iter() {
            account.apply(&envelope.payload);
        }
        Ok(Some(account))
    }

    async fn save(
        &self,
        aggregate: &BankAccount,
        events: Vec<BankAccountEvent>,
        context: BusinessContext,
    ) -> Result<Vec<EventEnvelope<BankAccount>>, BankAccountError> {
        let envelopes: Vec<EventEnvelope<BankAccount>> = events
            .into_iter()
            .map(|e| EventEnvelope::new(aggregate.id(), e, context.clone()))
            .collect();

        let last_seq = self
            .event_repo
            .get_events::<BankAccount>(aggregate.id())
            .await?
            .len();
        let serialized = serialize_events(&envelopes, last_seq + 1)?;
        self.event_repo.save::<BankAccount>(&serialized)?;

        Ok(envelopes)
    }
}

// ============================================================================
// 辅助函数：从 EventEnvelope 转换为 SerializedEvent
// ============================================================================

fn serialize_events<A: Aggregate>(
    envelopes: &[EventEnvelope<A>],
    start_sequence: usize,
) -> Result<Vec<SerializedEvent>> {
    envelopes
        .iter()
        .enumerate()
        .map(|(i, env)| SerializedEvent::from_envelope(env, start_sequence + i))
        .collect()
}

fn deserialize_events<A: Aggregate>(
    serialized: &[SerializedEvent],
) -> Result<Vec<EventEnvelope<A>>> {
    serialized.iter().map(|se| se.to_envelope::<A>()).collect()
}

// ============================================================================
// 主函数：演示使用
// ============================================================================

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let event_repo = InMemoryEventRepository::default();
    let repo = BankAccountRepository::new(event_repo.clone());
    let root = AggregateRoot::<BankAccount, _>::new(repo.clone());
    let account_id = "account-001".to_string();

    println!("=== EventRepository 示例（使用 AggregateRoot）===\n");

    // 使用 AggregateRoot 执行命令
    println!("--- 使用 AggregateRoot 执行命令 ---");

    // 存款
    let events = root
        .execute(
            &account_id,
            BankAccountCommand::Deposit { amount: 1000 },
            BusinessContext::default(),
        )
        .await?;
    println!("✅ 存款 +1000, 产生 {} 个事件", events.len());

    // 取款
    let events = root
        .execute(
            &account_id,
            BankAccountCommand::Withdraw { amount: 300 },
            BusinessContext::default(),
        )
        .await?;
    println!("✅ 取款 -300, 产生 {} 个事件", events.len());

    // 锁定账户
    let events = root
        .execute(
            &account_id,
            BankAccountCommand::Lock,
            BusinessContext::default(),
        )
        .await?;
    println!("✅ 锁定账户, 产生 {} 个事件", events.len());

    // 解锁账户
    let events = root
        .execute(
            &account_id,
            BankAccountCommand::Unlock,
            BusinessContext::default(),
        )
        .await?;
    println!("✅ 解锁账户, 产生 {} 个事件\n", events.len());

    // 直接使用 EventRepository 查询
    println!("--- 使用 EventRepository 查询事件 ---");
    let all_events = event_repo.get_events::<BankAccount>(&account_id).await?;
    println!("共 {} 个事件:", all_events.len());
    for event in &all_events {
        println!(
            "  序号: {}, 类型: {}, 版本: {}",
            event.sequence, event.event_type, event.event_version
        );
    }

    // 查询增量事件（从版本1之后）
    println!("\n--- 查询增量事件（version > 1）---");
    let incremental = event_repo
        .get_last_events::<BankAccount>(&account_id, 1)
        .await?;
    println!("共 {} 个增量事件:", incremental.len());
    for event in &incremental {
        println!(
            "  序号: {}, 类型: {}, 版本: {}",
            event.sequence, event.event_type, event.event_version
        );
    }

    // 使用 AggregateRepository 重新加载聚合
    println!("\n--- 使用 AggregateRepository 重新加载聚合 ---");
    let loaded_account = repo.load(&account_id).await?.unwrap();
    println!(
        "账户ID: {}, 余额: {}, 版本: {}, 锁定: {}",
        loaded_account.id(),
        loaded_account.balance,
        loaded_account.version(),
        loaded_account.is_locked
    );

    Ok(())
}
