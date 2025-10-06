/// Event Upcasting 示例
/// 演示如何使用 EventUpcaster 和 EventUpcasterChain 处理事件版本升级
///
/// 场景：银行账户的存款和取款事件经历多次版本升级
///
/// 存款事件演进：
/// - v1: account.credited { amount: i64 }
/// - v2: account.credited { amount: i64, currency: String } - 添加货币字段
/// - v3: account.credited { minor_units: i64, currency: String } - 金额改为最小单位（分）
/// - v4: account.deposited { minor_units: i64, currency: String } - 重命名为 deposited
///
/// 取款事件演进：
/// - v1: account.debited { amount: i64 }
/// - v2: account.debited { amount: i64, currency: String } - 添加货币字段
/// - v3: account.debited { minor_units: i64, currency: String } - 金额改为最小单位（分）
/// - v4: account.withdrew { minor_units: i64, currency: String } - 重命名为 withdrew
use anyhow::Result;
use ddd::aggregate::Aggregate;
use ddd::domain_event::{BusinessContext, DomainEvent, EventEnvelope};
use ddd::event_upcaster::{EventUpcaster, EventUpcasterChain, EventUpcasterResult};
use ddd::persist::{SerializedEvent, deserialize_events, serialize_events};
use ddd_macros::{aggregate, event};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use ulid::Ulid;

// ============================================================================
// 领域模型定义
// ============================================================================

#[aggregate]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct BankAccount {
    balance_minor_units: i64, // 余额（分）
    currency: String,
}

#[derive(Debug)]
enum BankAccountError {
    InvalidAmount,
    SerializationError(String),
}

impl Display for BankAccountError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidAmount => write!(f, "invalid amount"),
            Self::SerializationError(msg) => write!(f, "serialization error: {}", msg),
        }
    }
}

impl std::error::Error for BankAccountError {}

impl From<serde_json::Error> for BankAccountError {
    fn from(err: serde_json::Error) -> Self {
        Self::SerializationError(err.to_string())
    }
}

impl From<anyhow::Error> for BankAccountError {
    fn from(err: anyhow::Error) -> Self {
        Self::SerializationError(err.to_string())
    }
}

#[derive(Debug)]
#[allow(dead_code)]
enum BankAccountCommand {
    Credit { minor_units: i64, currency: String },
}

// 当前版本的事件（v4）
#[event]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum BankAccountEvent {
    #[serde(rename = "account.deposited")]
    Deposited { minor_units: i64, currency: String },

    #[serde(rename = "account.withdrew")]
    Withdrew { minor_units: i64, currency: String },
}

impl DomainEvent for BankAccountEvent {
    fn event_id(&self) -> String {
        match self {
            BankAccountEvent::Deposited { id, .. }
            | BankAccountEvent::Withdrew { id, .. } => id.clone(),
        }
    }
    fn event_type(&self) -> String {
        match self {
            BankAccountEvent::Deposited { .. } => "account.deposited",
            BankAccountEvent::Withdrew { .. } => "account.withdrew",
        }
        .to_string()
    }

    fn event_version(&self) -> usize {
        // 返回事件模式版本，所有 v4 事件都返回 4
        4
    }

    fn aggregate_version(&self) -> usize {
        match self {
            BankAccountEvent::Deposited { aggregate_version, .. }
            | BankAccountEvent::Withdrew { aggregate_version, .. } => *aggregate_version,
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
            balance_minor_units: 0,
            currency: "CNY".to_string(),
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
            BankAccountCommand::Credit {
                minor_units,
                currency,
            } => {
                if minor_units <= 0 {
                    return Err(BankAccountError::InvalidAmount);
                }
                Ok(vec![BankAccountEvent::Deposited {
                    id: Ulid::new().to_string(),
                    aggregate_version: self.version() + 1,
                    minor_units,
                    currency,
                }])
            }
        }
    }

    fn apply(&mut self, event: &Self::Event) {
        match event {
            BankAccountEvent::Deposited {
                aggregate_version,
                minor_units,
                currency,
                ..
            } => {
                // 单币种账户：如果是首次设置货币，则设置；否则应该保持一致
                if self.currency.is_empty() {
                    self.currency = currency.clone();
                }
                self.balance_minor_units += minor_units;
                // 如果事件中的 version 是占位值 0，则自动递增；否则使用事件中的值
                self.version = if *aggregate_version == 0 {
                    self.version + 1
                } else {
                    *aggregate_version
                };
            }
            BankAccountEvent::Withdrew {
                aggregate_version,
                minor_units,
                ..
            } => {
                // 取款：减少余额
                self.balance_minor_units -= minor_units;
                // 如果事件中的 version 是占位值 0，则自动递增；否则使用事件中的值
                self.version = if *aggregate_version == 0 {
                    self.version + 1
                } else {
                    *aggregate_version
                };
            }
        }
    }
}

// ============================================================================
// Event Upcasters
// ============================================================================

/// V1 -> V2: 为 account.credited 添加默认货币字段
struct AccountCreditedV1ToV2;

impl EventUpcaster for AccountCreditedV1ToV2 {
    fn applies(&self, event_type: &str, event_version: usize) -> bool {
        event_type == "account.credited" && event_version == 1
    }

    fn upcast(&self, event: SerializedEvent) -> Result<EventUpcasterResult> {
        let mut payload = event.payload().clone();
        let amount = payload
            .get("amount")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| anyhow::anyhow!("v1 missing amount"))?;

        println!(
            "  [Upcaster V1->V2] amount={} -> amount={}, currency=CNY",
            amount, amount
        );

        // 添加默认货币字段
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("currency".to_string(), serde_json::json!("CNY"));
        }

        let upgraded = SerializedEvent::builder()
            .event_id(event.event_id().to_string())
            .event_type(event.event_type().to_string())
            .event_version(2) // 升级到 v2
            .maybe_sequence_number(None)
            .aggregate_id(event.aggregate_id().to_string())
            .aggregate_type(event.aggregate_type().to_string())
            .aggregate_version(event.aggregate_version())
            .maybe_correlation_id(event.correlation_id().map(|s| s.to_string()))
            .maybe_causation_id(event.causation_id().map(|s| s.to_string()))
            .maybe_actor_type(event.actor_type().map(|s| s.to_string()))
            .maybe_actor_id(event.actor_id().map(|s| s.to_string()))
            .occurred_at(event.occurred_at())
            .payload(payload)
            .build();

        Ok(EventUpcasterResult::One(upgraded))
    }
}

/// V2 -> V3: 金额从元转换为分
struct AccountCreditedV2ToV3;

impl EventUpcaster for AccountCreditedV2ToV3 {
    fn applies(&self, event_type: &str, event_version: usize) -> bool {
        event_type == "account.credited" && event_version == 2
    }

    fn upcast(&self, event: SerializedEvent) -> Result<EventUpcasterResult> {
        let mut payload = event.payload().clone();
        let amount = payload
            .get("amount")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| anyhow::anyhow!("v2 missing amount"))?;
        let currency = payload
            .get("currency")
            .and_then(|v| v.as_str())
            .unwrap_or("CNY");

        let minor_units = amount * 100;

        println!(
            "  [Upcaster V2->V3] amount={} {} -> minor_units={} {}",
            amount, currency, minor_units, currency
        );

        // 替换 amount 为 minor_units
        if let Some(obj) = payload.as_object_mut() {
            obj.remove("amount");
            obj.insert("minor_units".to_string(), serde_json::json!(minor_units));
        }

        let upgraded = SerializedEvent::builder()
            .event_id(event.event_id().to_string())
            .event_type(event.event_type().to_string())
            .event_version(3) // 升级到 v3
            .maybe_sequence_number(None)
            .aggregate_id(event.aggregate_id().to_string())
            .aggregate_type(event.aggregate_type().to_string())
            .aggregate_version(event.aggregate_version())
            .maybe_correlation_id(event.correlation_id().map(|s| s.to_string()))
            .maybe_causation_id(event.causation_id().map(|s| s.to_string()))
            .maybe_actor_type(event.actor_type().map(|s| s.to_string()))
            .maybe_actor_id(event.actor_id().map(|s| s.to_string()))
            .occurred_at(event.occurred_at())
            .payload(payload)
            .build();

        Ok(EventUpcasterResult::One(upgraded))
    }
}

/// V3 -> V4: 重命名 account.credited 为 account.deposited
struct AccountCreditedV3ToV4;

impl EventUpcaster for AccountCreditedV3ToV4 {
    fn applies(&self, event_type: &str, event_version: usize) -> bool {
        event_type == "account.credited" && event_version == 3
    }

    fn upcast(&self, event: SerializedEvent) -> Result<EventUpcasterResult> {
        let payload = event.payload();
        let minor_units = payload
            .get("minor_units")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| anyhow::anyhow!("v3 missing minor_units"))?;
        let currency = payload
            .get("currency")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("v3 missing currency"))?;

        println!(
            "  [Upcaster V3->V4] Renaming account.credited to account.deposited ({} {})",
            minor_units, currency
        );

        let deposited_payload = serde_json::json!({
            "account.deposited": {
                "id": event.event_id(),
                "version": event.aggregate_version(),
                "minor_units": minor_units,
                "currency": currency,
            }
        });

        let deposited_event = SerializedEvent::builder()
            .event_id(event.event_id().to_string())
            .event_type("account.deposited".to_string())
            .event_version(4)
            .maybe_sequence_number(None)
            .aggregate_id(event.aggregate_id().to_string())
            .aggregate_type(event.aggregate_type().to_string())
            .aggregate_version(event.aggregate_version())
            .maybe_correlation_id(event.correlation_id().map(|s| s.to_string()))
            .maybe_causation_id(event.causation_id().map(|s| s.to_string()))
            .maybe_actor_type(event.actor_type().map(|s| s.to_string()))
            .maybe_actor_id(event.actor_id().map(|s| s.to_string()))
            .occurred_at(event.occurred_at())
            .payload(deposited_payload)
            .build();

        Ok(EventUpcasterResult::One(deposited_event))
    }
}

// ============================================================================
// 取款事件的 Upcasters
// ============================================================================

/// V1 -> V2: 为 account.debited 添加默认货币字段
struct AccountDebitedV1ToV2;

impl EventUpcaster for AccountDebitedV1ToV2 {
    fn applies(&self, event_type: &str, event_version: usize) -> bool {
        event_type == "account.debited" && event_version == 1
    }

    fn upcast(&self, event: SerializedEvent) -> Result<EventUpcasterResult> {
        let mut payload = event.payload().clone();
        let amount = payload
            .get("amount")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| anyhow::anyhow!("v1 missing amount"))?;

        println!(
            "  [Upcaster V1->V2] amount={} -> amount={}, currency=CNY (debited)",
            amount, amount
        );

        if let Some(obj) = payload.as_object_mut() {
            obj.insert("currency".to_string(), serde_json::json!("CNY"));
        }

        let upgraded = SerializedEvent::builder()
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
            .payload(payload)
            .build();

        Ok(EventUpcasterResult::One(upgraded))
    }
}

/// V2 -> V3: 金额从元转换为分 (debited)
struct AccountDebitedV2ToV3;

impl EventUpcaster for AccountDebitedV2ToV3 {
    fn applies(&self, event_type: &str, event_version: usize) -> bool {
        event_type == "account.debited" && event_version == 2
    }

    fn upcast(&self, event: SerializedEvent) -> Result<EventUpcasterResult> {
        let mut payload = event.payload().clone();
        let amount = payload
            .get("amount")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| anyhow::anyhow!("v2 missing amount"))?;
        let currency = payload
            .get("currency")
            .and_then(|v| v.as_str())
            .unwrap_or("CNY");

        let minor_units = amount * 100;

        println!(
            "  [Upcaster V2->V3] amount={} {} -> minor_units={} {} (debited)",
            amount, currency, minor_units, currency
        );

        if let Some(obj) = payload.as_object_mut() {
            obj.remove("amount");
            obj.insert("minor_units".to_string(), serde_json::json!(minor_units));
        }

        let upgraded = SerializedEvent::builder()
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
            .payload(payload)
            .build();

        Ok(EventUpcasterResult::One(upgraded))
    }
}

/// V3 -> V4: 重命名 account.debited 为 account.withdrew
struct AccountDebitedV3ToV4;

impl EventUpcaster for AccountDebitedV3ToV4 {
    fn applies(&self, event_type: &str, event_version: usize) -> bool {
        event_type == "account.debited" && event_version == 3
    }

    fn upcast(&self, event: SerializedEvent) -> Result<EventUpcasterResult> {
        let payload = event.payload();
        let minor_units = payload
            .get("minor_units")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| anyhow::anyhow!("v3 missing minor_units"))?;
        let currency = payload
            .get("currency")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("v3 missing currency"))?;

        println!(
            "  [Upcaster V3->V4] Renaming account.debited to account.withdrew ({} {})",
            minor_units, currency
        );

        let withdrew_payload = serde_json::json!({
            "account.withdrew": {
                "id": event.event_id(),
                "version": event.aggregate_version(),
                "minor_units": minor_units,
                "currency": currency,
            }
        });

        let withdrew_event = SerializedEvent::builder()
            .event_id(event.event_id().to_string())
            .event_type("account.withdrew".to_string())
            .event_version(4)
            .maybe_sequence_number(None)
            .aggregate_id(event.aggregate_id().to_string())
            .aggregate_type(event.aggregate_type().to_string())
            .aggregate_version(event.aggregate_version())
            .maybe_correlation_id(event.correlation_id().map(|s| s.to_string()))
            .maybe_causation_id(event.causation_id().map(|s| s.to_string()))
            .maybe_actor_type(event.actor_type().map(|s| s.to_string()))
            .maybe_actor_id(event.actor_id().map(|s| s.to_string()))
            .occurred_at(event.occurred_at())
            .payload(withdrew_payload)
            .build();

        Ok(EventUpcasterResult::One(withdrew_event))
    }
}

// ============================================================================
// 通用事件创建函数
// ============================================================================

/// 创建存款事件 (支持v1-v4所有版本)
fn create_deposit(
    id: &str,
    ver: usize,
    yuan: Option<i64>,
    cents: Option<i64>,
    currency: Option<&str>,
) -> SerializedEvent {
    let eid = Ulid::new().to_string();
    let aver: usize = 0;
    let (event_type, payload) = match ver {
        1 => (
            "account.credited",
            serde_json::json!({
                "id": eid,
                "version": aver,
                "amount": yuan.unwrap()
            }),
        ),
        2 => (
            "account.credited",
            serde_json::json!({
                "id": eid,
                "version": aver,
                "amount": yuan.unwrap(),
                "currency": currency.unwrap()
            }),
        ),
        3 => (
            "account.credited",
            serde_json::json!({
                "id": eid,
                "version": aver,
                "minor_units": cents.unwrap(),
                "currency": currency.unwrap()
            }),
        ),
        4 => (
            "account.deposited",
            serde_json::json!({
                "account.deposited": {
                    "id": eid,
                    "version": aver,
                    "minor_units": cents.unwrap(),
                    "currency": currency.unwrap()
                }
            }),
        ),
        _ => panic!("Unsupported version"),
    };
    SerializedEvent::builder()
        .event_id(eid)
        .event_type(event_type.to_string())
        .event_version(ver)
        .maybe_sequence_number(None)
        .aggregate_id(id.to_string())
        .aggregate_type("bank_account".to_string())
        .aggregate_version(aver)
        .occurred_at(chrono::Utc::now())
        .payload(payload)
        .build()
}

/// 创建取款事件 (支持v1-v4所有版本)
fn create_withdraw(
    id: &str,
    ver: usize,
    yuan: Option<i64>,
    cents: Option<i64>,
    currency: Option<&str>,
) -> SerializedEvent {
    let eid = Ulid::new().to_string();
    let aver: usize = 0;
    let (event_type, payload) = match ver {
        1 => (
            "account.debited",
            serde_json::json!({
                "id": eid,
                "version": aver,
                "amount": yuan.unwrap()
            }),
        ),
        2 => (
            "account.debited",
            serde_json::json!({
                "id": eid,
                "version": aver,
                "amount": yuan.unwrap(),
                "currency": currency.unwrap()
            }),
        ),
        3 => (
            "account.debited",
            serde_json::json!({
                "id": eid,
                "version": aver,
                "minor_units": cents.unwrap(),
                "currency": currency.unwrap()
            }),
        ),
        4 => (
            "account.withdrew",
            serde_json::json!({
                "account.withdrew": {
                    "id": eid,
                    "version": aver,
                    "minor_units": cents.unwrap(),
                    "currency": currency.unwrap()
                }
            }),
        ),
        _ => panic!("Unsupported version"),
    };
    SerializedEvent::builder()
        .event_id(eid)
        .event_type(event_type.to_string())
        .event_version(ver)
        .maybe_sequence_number(None)
        .aggregate_id(id.to_string())
        .aggregate_type("bank_account".to_string())
        .aggregate_version(aver)
        .occurred_at(chrono::Utc::now())
        .payload(payload)
        .build()
}

// ============================================================================
// 主函数
// ============================================================================

fn main() -> Result<()> {
    println!("=== Event Upcasting 示例 ===\n");

    let account_id = "acc-001";

    // 构建 Upcaster Chain
    let chain = EventUpcasterChain::new()
        .add(AccountCreditedV1ToV2)
        .add(AccountCreditedV2ToV3)
        .add(AccountCreditedV3ToV4)
        .add(AccountDebitedV1ToV2)
        .add(AccountDebitedV2ToV3)
        .add(AccountDebitedV3ToV4);

    // 模拟从数据库读取历史事件（按时间顺序：v1 → v2 → v3 → v4）
    println!("原始事件（混合版本）:");
    let serialized_events = vec![
        // 早期事件 (v1)
        create_deposit(account_id, 1, Some(100), None, None), // v1: 存入 100 元
        create_withdraw(account_id, 1, Some(30), None, None), // v1: 取出 30 元
        // 添加货币字段后 (v2)
        create_deposit(account_id, 2, Some(50), None, Some("CNY")), // v2: 存入 50 元
        create_withdraw(account_id, 2, Some(20), None, Some("CNY")), // v2: 取出 20 元
        create_withdraw(account_id, 2, Some(5), None, Some("CNY")), // v2: 取出 5 元
        // 改用分作为单位后 (v3)
        create_deposit(account_id, 3, None, Some(8000), Some("CNY")), // v3: 存入 80 元 (8000分)
        create_withdraw(account_id, 3, None, Some(1000), Some("CNY")), // v3: 取出 10 元 (1000分)
        create_deposit(account_id, 3, None, Some(2000), Some("CNY")), // v3: 存入 20 元 (2000分)
        // 最新版本 (v4 - 重命名事件)
        create_deposit(account_id, 4, None, Some(5000), Some("CNY")), // v4: 存入 50 元 (5000分)
        create_withdraw(account_id, 4, None, Some(3000), Some("CNY")), // v4: 取出 30 元 (3000分)
    ];

    for (i, se) in serialized_events.iter().enumerate() {
        println!("  {}. {} v{}", i + 1, se.event_type(), se.event_version());
    }
    println!();

    // 反序列化并自动升级
    println!("升级过程:");

    let upcasted_events: Vec<EventEnvelope<BankAccount>> =
        deserialize_events(&chain, serialized_events)?;

    println!(
        "✅ 升级完成: {} 个事件全部升级到 v4\n",
        upcasted_events.len()
    );

    // 应用事件重建聚合状态
    println!("应用事件重建状态:");
    let mut account = BankAccount::new(account_id.to_string());

    for envelope in &upcasted_events {
        account.apply(&envelope.payload);
    }

    println!(
        "  余额: {} 分 ({:.2} 元), 版本: {}\n",
        account.balance_minor_units,
        account.balance_minor_units as f64 / 100.0,
        account.version()
    );

    // 演示新事件的序列化
    println!("新事件序列化演示:");
    let new_event = BankAccountEvent::Deposited {
        id: Ulid::new().to_string(),
        aggregate_version: account.version() + 1,
        minor_units: 2000,
        currency: "CNY".to_string(),
    };

    let new_envelope: EventEnvelope<BankAccount> = EventEnvelope::new(
        &account_id.to_string(),
        new_event,
        BusinessContext::default(),
    );

    let serialized = serialize_events(&[new_envelope])?;
    println!(
        "  {} v{} → SerializedEvent\n",
        serialized[0].event_type(),
        serialized[0].event_version()
    );

    // 总结
    println!("=== 应用场景总结 ===");
    println!("• 加载: repository.get_events() → SerializedEvent[]");
    println!("• 升级: deserialize_events(&chain, events) → 自动升级到 v4");
    println!("• 重建: aggregate.apply() → 恢复聚合状态");
    println!("• 存储: serialize_events() → 新事件持久化");
    println!("\n✨ 优势: 历史事件自动升级，业务代码仅处理最新版本");

    Ok(())
}
