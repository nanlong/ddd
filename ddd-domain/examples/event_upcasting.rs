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
use anyhow::Result as AnyResult;
use async_trait::async_trait;
use ddd_domain::aggregate::Aggregate;
use ddd_domain::domain_event::{BusinessContext, EventEnvelope};
use ddd_domain::entity::Entity;
use ddd_domain::error::{DomainError, DomainResult};
use ddd_domain::event_upcaster::{EventUpcaster, EventUpcasterChain, EventUpcasterResult};
use ddd_domain::persist::{
    AggregateRepository, EventRepository, EventSourcedRepo, SerializedEvent, SerializedSnapshot,
    SnapshotPolicy, SnapshotPolicyRepo, SnapshotRepository, SnapshotRepositoryWithPolicy,
    serialize_events,
};
use ddd_macros::{domain_event, entity};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use ulid::Ulid;

// ============================================================================
// 领域模型定义
// ============================================================================

#[entity]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct BankAccount {
    balance_minor_units: i64, // 余额（分）
    currency: String,
}

#[derive(Debug)]
#[allow(dead_code)]
enum BankAccountCommand {
    Credit { minor_units: i64, currency: String },
}

// 当前版本的事件（v4）
#[domain_event]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum BankAccountEvent {
    #[event(event_type = "account.deposited", event_version = 4)]
    Deposited { minor_units: i64, currency: String },

    #[event(event_type = "account.withdrew", event_version = 4)]
    Withdrew { minor_units: i64, currency: String },
}

impl Aggregate for BankAccount {
    const TYPE: &'static str = "bank_account";
    type Command = BankAccountCommand;
    type Event = BankAccountEvent;
    type Error = DomainError;

    fn execute(&self, command: Self::Command) -> Result<Vec<Self::Event>, Self::Error> {
        match command {
            BankAccountCommand::Credit {
                minor_units,
                currency,
            } => {
                if minor_units <= 0 {
                    return Err(DomainError::InvalidCommand {
                        reason: "amount must be positive".to_string(),
                    });
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

    fn upcast(&self, event: SerializedEvent) -> DomainResult<EventUpcasterResult> {
        let mut payload = event.payload().clone();
        let amount = payload
            .get("amount")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| DomainError::UpcastFailed {
                event_type: event.event_type().to_string(),
                from_version: event.event_version(),
                stage: Some("AccountCreditedV1ToV2"),
                reason: "v1 missing amount".to_string(),
            })?;

        println!(
            "  [Upcaster V1->V2] amount={} -> amount={}, currency=CNY",
            amount, amount
        );

        // 添加默认货币字段
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("currency".to_string(), serde_json::json!("CNY"));
        }

        // 重建 BusinessContext 以保留原始事件的业务上下文
        let business_context = BusinessContext::builder()
            .maybe_correlation_id(event.correlation_id().map(|s| s.to_string()))
            .maybe_causation_id(event.causation_id().map(|s| s.to_string()))
            .maybe_actor_type(event.actor_type().map(|s| s.to_string()))
            .maybe_actor_id(event.actor_id().map(|s| s.to_string()))
            .build();

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
            .context(serde_json::to_value(&business_context)?)
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

    fn upcast(&self, event: SerializedEvent) -> DomainResult<EventUpcasterResult> {
        let mut payload = event.payload().clone();
        let amount = payload
            .get("amount")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| DomainError::UpcastFailed {
                event_type: event.event_type().to_string(),
                from_version: event.event_version(),
                stage: Some("AccountCreditedV2ToV3"),
                reason: "v2 missing amount".to_string(),
            })?;
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

        // 重建 BusinessContext 以保留原始事件的业务上下文
        let business_context = BusinessContext::builder()
            .maybe_correlation_id(event.correlation_id().map(|s| s.to_string()))
            .maybe_causation_id(event.causation_id().map(|s| s.to_string()))
            .maybe_actor_type(event.actor_type().map(|s| s.to_string()))
            .maybe_actor_id(event.actor_id().map(|s| s.to_string()))
            .build();

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
            .context(serde_json::to_value(&business_context)?)
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

    fn upcast(&self, event: SerializedEvent) -> DomainResult<EventUpcasterResult> {
        let payload = event.payload();
        let minor_units = payload
            .get("minor_units")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| DomainError::UpcastFailed {
                event_type: event.event_type().to_string(),
                from_version: event.event_version(),
                stage: Some("AccountCreditedV3ToV4"),
                reason: "v3 missing minor_units".to_string(),
            })?;
        let currency = payload
            .get("currency")
            .and_then(|v| v.as_str())
            .ok_or_else(|| DomainError::UpcastFailed {
                event_type: event.event_type().to_string(),
                from_version: event.event_version(),
                stage: Some("AccountCreditedV3ToV4"),
                reason: "v3 missing currency".to_string(),
            })?;

        println!(
            "  [Upcaster V3->V4] Renaming account.credited to account.deposited ({} {})",
            minor_units, currency
        );

        let deposited_payload = serde_json::json!({
            "Deposited": {
                "id": event.event_id(),
                "aggregate_version": event.aggregate_version(),
                "minor_units": minor_units,
                "currency": currency,
            }
        });

        // 重建 BusinessContext 以保留原始事件的业务上下文
        let business_context = BusinessContext::builder()
            .maybe_correlation_id(event.correlation_id().map(|s| s.to_string()))
            .maybe_causation_id(event.causation_id().map(|s| s.to_string()))
            .maybe_actor_type(event.actor_type().map(|s| s.to_string()))
            .maybe_actor_id(event.actor_id().map(|s| s.to_string()))
            .build();

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
            .context(serde_json::to_value(&business_context)?)
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

    fn upcast(&self, event: SerializedEvent) -> DomainResult<EventUpcasterResult> {
        let mut payload = event.payload().clone();
        let amount = payload
            .get("amount")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| DomainError::UpcastFailed {
                event_type: event.event_type().to_string(),
                from_version: event.event_version(),
                stage: Some("AccountDebitedV1ToV2"),
                reason: "v1 missing amount".to_string(),
            })?;

        println!(
            "  [Upcaster V1->V2] amount={} -> amount={}, currency=CNY (debited)",
            amount, amount
        );

        if let Some(obj) = payload.as_object_mut() {
            obj.insert("currency".to_string(), serde_json::json!("CNY"));
        }

        // 重建 BusinessContext 以保留原始事件的业务上下文
        let business_context = BusinessContext::builder()
            .maybe_correlation_id(event.correlation_id().map(|s| s.to_string()))
            .maybe_causation_id(event.causation_id().map(|s| s.to_string()))
            .maybe_actor_type(event.actor_type().map(|s| s.to_string()))
            .maybe_actor_id(event.actor_id().map(|s| s.to_string()))
            .build();

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
            .context(serde_json::to_value(&business_context)?)
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

    fn upcast(&self, event: SerializedEvent) -> DomainResult<EventUpcasterResult> {
        let mut payload = event.payload().clone();
        let amount = payload
            .get("amount")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| DomainError::UpcastFailed {
                event_type: event.event_type().to_string(),
                from_version: event.event_version(),
                stage: Some("AccountDebitedV2ToV3"),
                reason: "v2 missing amount".to_string(),
            })?;
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

        // 重建 BusinessContext 以保留原始事件的业务上下文
        let business_context = BusinessContext::builder()
            .maybe_correlation_id(event.correlation_id().map(|s| s.to_string()))
            .maybe_causation_id(event.causation_id().map(|s| s.to_string()))
            .maybe_actor_type(event.actor_type().map(|s| s.to_string()))
            .maybe_actor_id(event.actor_id().map(|s| s.to_string()))
            .build();

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
            .context(serde_json::to_value(&business_context)?)
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

    fn upcast(&self, event: SerializedEvent) -> DomainResult<EventUpcasterResult> {
        let payload = event.payload();
        let minor_units = payload
            .get("minor_units")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| DomainError::UpcastFailed {
                event_type: event.event_type().to_string(),
                from_version: event.event_version(),
                stage: Some("AccountDebitedV3ToV4"),
                reason: "v3 missing minor_units".to_string(),
            })?;
        let currency = payload
            .get("currency")
            .and_then(|v| v.as_str())
            .ok_or_else(|| DomainError::UpcastFailed {
                event_type: event.event_type().to_string(),
                from_version: event.event_version(),
                stage: Some("AccountDebitedV3ToV4"),
                reason: "v3 missing currency".to_string(),
            })?;

        println!(
            "  [Upcaster V3->V4] Renaming account.debited to account.withdrew ({} {})",
            minor_units, currency
        );

        let withdrew_payload = serde_json::json!({
            "Withdrew": {
                "id": event.event_id(),
                "aggregate_version": event.aggregate_version(),
                "minor_units": minor_units,
                "currency": currency,
            }
        });

        // 重建 BusinessContext 以保留原始事件的业务上下文
        let business_context = BusinessContext::builder()
            .maybe_correlation_id(event.correlation_id().map(|s| s.to_string()))
            .maybe_causation_id(event.causation_id().map(|s| s.to_string()))
            .maybe_actor_type(event.actor_type().map(|s| s.to_string()))
            .maybe_actor_id(event.actor_id().map(|s| s.to_string()))
            .build();

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
            .context(serde_json::to_value(&business_context)?)
            .build();

        Ok(EventUpcasterResult::One(withdrew_event))
    }
}

// ============================================================================
// 内存仓储实现（示例）
// ============================================================================

#[derive(Default, Clone)]
struct InMemoryEventRepository {
    events: Arc<Mutex<HashMap<String, Vec<SerializedEvent>>>>,
}

#[async_trait]
impl EventRepository for InMemoryEventRepository {
    async fn get_events<A: Aggregate>(
        &self,
        aggregate_id: &A::Id,
    ) -> DomainResult<Vec<SerializedEvent>> {
        let store = self.events.lock().unwrap();
        Ok(store
            .get(&aggregate_id.to_string())
            .cloned()
            .unwrap_or_default())
    }

    async fn get_last_events<A: Aggregate>(
        &self,
        aggregate_id: &A::Id,
        last_version: usize,
    ) -> DomainResult<Vec<SerializedEvent>> {
        let store = self.events.lock().unwrap();
        Ok(store
            .get(&aggregate_id.to_string())
            .map(|events| {
                events
                    .iter()
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

        let mut store = self.events.lock().unwrap();
        let aggregate_id = events[0].aggregate_id().to_string();
        let entry = store.entry(aggregate_id).or_default();
        entry.extend_from_slice(&events);

        Ok(())
    }
}

type SnapshotsMap = HashMap<(String, String), Vec<SerializedSnapshot>>;

#[derive(Clone)]
struct InMemorySnapshotRepository {
    // 内存存储快照，策略由外层装饰器控制
    snapshots: Arc<Mutex<SnapshotsMap>>,
}

impl Default for InMemorySnapshotRepository {
    fn default() -> Self {
        Self {
            snapshots: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl SnapshotRepository for InMemorySnapshotRepository {
    async fn get_snapshot<A: Aggregate>(
        &self,
        aggregate_id: &A::Id,
        version: Option<usize>,
    ) -> DomainResult<Option<SerializedSnapshot>> {
        let store = self.snapshots.lock().unwrap();
        let key = (A::TYPE.to_string(), aggregate_id.to_string());

        if let Some(snaps) = store.get(&key) {
            match version {
                Some(target) => Ok(snaps
                    .iter()
                    .filter(|s| s.aggregate_version() <= target)
                    .max_by_key(|s| s.aggregate_version())
                    .cloned()),
                None => Ok(snaps.last().cloned()),
            }
        } else {
            Ok(None)
        }
    }

    async fn save<A: Aggregate>(&self, aggregate: &A) -> DomainResult<()> {
        let snapshot = SerializedSnapshot::from_aggregate(aggregate)?;
        let mut store = self.snapshots.lock().unwrap();
        let key = (A::TYPE.to_string(), aggregate.id().to_string());
        let entry = store.entry(key).or_default();
        entry.push(snapshot);
        entry.sort_by_key(|s| s.aggregate_version());

        Ok(())
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
                "aggregate_version": aver,
                "amount": yuan.unwrap()
            }),
        ),
        2 => (
            "account.credited",
            serde_json::json!({
                "id": eid,
                "aggregate_version": aver,
                "amount": yuan.unwrap(),
                "currency": currency.unwrap()
            }),
        ),
        3 => (
            "account.credited",
            serde_json::json!({
                "id": eid,
                "aggregate_version": aver,
                "minor_units": cents.unwrap(),
                "currency": currency.unwrap()
            }),
        ),
        4 => (
            "account.deposited",
            serde_json::json!({
                "Deposited": {
                    "id": eid,
                    "aggregate_version": aver,
                    "minor_units": cents.unwrap(),
                    "currency": currency.unwrap()
                }
            }),
        ),
        _ => panic!("Unsupported version"),
    };
    let biz = BusinessContext::builder()
        .maybe_correlation_id(Some(format!("cor-{id}")))
        .maybe_causation_id(Some(format!("cau-{id}")))
        .maybe_actor_type(Some("user".into()))
        .maybe_actor_id(Some("u-1".into()))
        .build();

    SerializedEvent::builder()
        .event_id(eid)
        .event_type(event_type.to_string())
        .event_version(ver)
        .maybe_sequence_number(None)
        .aggregate_id(id.to_string())
        .aggregate_type("bank_account".to_string())
        .aggregate_version(aver)
        .correlation_id(format!("cor-{id}"))
        .causation_id(format!("cau-{id}"))
        .actor_type("user".into())
        .actor_id("u-1".into())
        .occurred_at(chrono::Utc::now())
        .payload(payload)
        .context(serde_json::to_value(&biz).expect("serialize BusinessContext"))
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
                "aggregate_version": aver,
                "amount": yuan.unwrap()
            }),
        ),
        2 => (
            "account.debited",
            serde_json::json!({
                "id": eid,
                "aggregate_version": aver,
                "amount": yuan.unwrap(),
                "currency": currency.unwrap()
            }),
        ),
        3 => (
            "account.debited",
            serde_json::json!({
                "id": eid,
                "aggregate_version": aver,
                "minor_units": cents.unwrap(),
                "currency": currency.unwrap()
            }),
        ),
        4 => (
            "account.withdrew",
            serde_json::json!({
                "Withdrew": {
                    "id": eid,
                    "aggregate_version": aver,
                    "minor_units": cents.unwrap(),
                    "currency": currency.unwrap()
                }
            }),
        ),
        _ => panic!("Unsupported version"),
    };
    let biz = BusinessContext::builder()
        .maybe_correlation_id(Some(format!("cor-{id}")))
        .maybe_causation_id(Some(format!("cau-{id}")))
        .maybe_actor_type(Some("user".into()))
        .maybe_actor_id(Some("u-1".into()))
        .build();

    SerializedEvent::builder()
        .event_id(eid)
        .event_type(event_type.to_string())
        .event_version(ver)
        .maybe_sequence_number(None)
        .aggregate_id(id.to_string())
        .aggregate_type("bank_account".to_string())
        .aggregate_version(aver)
        .correlation_id(format!("cor-{id}"))
        .causation_id(format!("cau-{id}"))
        .actor_type("user".into())
        .actor_id("u-1".into())
        .occurred_at(chrono::Utc::now())
        .payload(payload)
        .context(serde_json::to_value(&biz).expect("serialize BusinessContext"))
        .build()
}

// ============================================================================
// 主函数
// ============================================================================

#[tokio::main(flavor = "current_thread")]
async fn main() -> AnyResult<()> {
    println!("=== Event Upcasting 示例 ===\n");

    let account_id = "acc-001".to_string();

    // 构建 Upcaster Chain，并包裹在 Arc 中便于共享
    let upcaster_chain: Arc<EventUpcasterChain> = Arc::new(
        vec![
            Arc::new(AccountCreditedV1ToV2) as Arc<dyn EventUpcaster>,
            Arc::new(AccountCreditedV2ToV3) as Arc<dyn EventUpcaster>,
            Arc::new(AccountCreditedV3ToV4) as Arc<dyn EventUpcaster>,
            Arc::new(AccountDebitedV1ToV2) as Arc<dyn EventUpcaster>,
            Arc::new(AccountDebitedV2ToV3) as Arc<dyn EventUpcaster>,
            Arc::new(AccountDebitedV3ToV4) as Arc<dyn EventUpcaster>,
        ]
        .into_iter()
        .collect(),
    );

    let event_repo = Arc::new(InMemoryEventRepository::default());
    // 通过装饰器为基础仓储附加快照策略，确保 save 时自动评估策略
    let snapshot_repo = Arc::new(SnapshotRepositoryWithPolicy::new(
        Arc::new(InMemorySnapshotRepository::default()),
        SnapshotPolicy::Every(1),
    ));

    // 构造历史事件（混合多个版本）并写入事件仓储
    println!("原始事件（混合版本）:");
    let historical_events = vec![
        create_deposit(&account_id, 1, Some(100), None, None), // v1: 存入 100 元
        create_withdraw(&account_id, 1, Some(30), None, None), // v1: 取出 30 元
        create_deposit(&account_id, 2, Some(50), None, Some("CNY")), // v2: 存入 50 元
        create_withdraw(&account_id, 2, Some(20), None, Some("CNY")), // v2: 取出 20 元
        create_withdraw(&account_id, 2, Some(5), None, Some("CNY")), // v2: 取出 5 元
        create_deposit(&account_id, 3, None, Some(8000), Some("CNY")), // v3: 存入 80 元 (8000分)
        create_withdraw(&account_id, 3, None, Some(1000), Some("CNY")), // v3: 取出 10 元 (1000分)
        create_deposit(&account_id, 3, None, Some(2000), Some("CNY")), // v3: 存入 20 元 (2000分)
        create_deposit(&account_id, 4, None, Some(5000), Some("CNY")), // v4: 存入 50 元 (5000分)
        create_withdraw(&account_id, 4, None, Some(3000), Some("CNY")), // v4: 取出 30 元 (3000分)
    ];

    for (i, se) in historical_events.iter().enumerate() {
        println!("  {}. {} v{}", i + 1, se.event_type(), se.event_version());
    }
    println!();

    event_repo.save(historical_events).await?;

    // 使用 EventSourcedRepo 自动上抬并重建聚合
    println!("使用 EventSourcedRepo 重建聚合:");
    let account: BankAccount =
        match EventSourcedRepo::new(event_repo.clone(), upcaster_chain.clone())
            .load(&account_id)
            .await?
        {
            Some(aggregate) => aggregate,
            None => {
                println!("  ⚠️ 仓储中没有找到事件");
                return Ok(());
            }
        };

    println!(
        "  ✅ 升级完成: 余额 {} 分 ({:.2} 元), 版本 {}",
        account.balance_minor_units,
        account.balance_minor_units as f64 / 100.0,
        account.version()
    );

    // 保存快照，随后追加增量事件模拟快照之后的演进
    snapshot_repo.save(&account).await?;
    println!("  💾 已保存快照 (版本 {})", account.version());

    let incremental_events = vec![
        create_withdraw(&account_id, 2, Some(10), None, Some("CNY")), // v2: 追加取款 10 元
        create_deposit(&account_id, 3, None, Some(1500), Some("CNY")), // v3: 追加存款 15 元 (1500分)
    ];
    println!(
        "  ➕ 追加 {} 个增量事件（快照之后）",
        incremental_events.len()
    );
    event_repo.save(incremental_events).await?;

    // 使用 SnapshottingAggregateRepository：先加载快照，再上抬快照后的增量事件
    let account_after_snapshot: BankAccount = match SnapshotPolicyRepo::new(
        event_repo.clone(),
        snapshot_repo.clone(),
        upcaster_chain.clone(),
    )
    .load(&account_id)
    .await?
    {
        Some(aggregate) => aggregate,
        None => {
            println!("  ⚠️ 快照仓储中没有聚合");
            return Ok(());
        }
    };

    println!(
        "  🔁 SnapshottingAggregateRepository 加载后: 余额 {} 分 ({:.2} 元), 版本 {}\n",
        account_after_snapshot.balance_minor_units,
        account_after_snapshot.balance_minor_units as f64 / 100.0,
        account_after_snapshot.version()
    );

    // 演示新事件的序列化（始终使用最新版本的事件结构）
    println!("新事件序列化演示:");
    let new_event = BankAccountEvent::Deposited {
        id: Ulid::new().to_string(),
        aggregate_version: account_after_snapshot.version() + 1,
        minor_units: 2000,
        currency: "CNY".to_string(),
    };

    let new_envelope: EventEnvelope<BankAccount> =
        EventEnvelope::new(&account_id, new_event, BusinessContext::default());

    let serialized = serialize_events(&[new_envelope])?;
    println!(
        "  {} v{} → SerializedEvent\n",
        serialized[0].event_type(),
        serialized[0].event_version()
    );

    // 总结
    println!("=== 应用场景总结 ===");
    println!("• 仓储: EventSourcedRepo::load() → 历史事件自动升级");
    println!("• 快照: SnapshottingAggregateRepository::load() → 快照 + 增量事件一体化恢复");
    println!("• 存储: EventRepository::save() / serialize_events() → 新事件持久化");
    println!("\n✨ 优势: 历史事件自动升级，业务代码仅处理最新版本");

    Ok(())
}
