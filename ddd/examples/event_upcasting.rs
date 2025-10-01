/// 事件升级示例
/// 1. 定义事件信封（EventEnvelope）
/// 2. 实现多个 Upcaster
/// 3. 链式组合 UpcasterChain
/// 4. 读取原始事件，统一升级
use anyhow::Result;
use ddd::upcast::{UpcastResult, Upcaster, UpcasterChain};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub event_type: String,  // 例如 "account.credited"
    pub schema_version: u16, // 例如 1,2,3...
    pub aggregate_id: String,
    pub aggregate_version: u64,
    pub global_seq: u64,
    pub payload: Value,  // 原始 JSON 负载
    pub metadata: Value, // 例如 correlation_id, causation_id, user_id 等
    pub timestamp: i64,
}

/// 事件：account.credited v1: { "amount": 100 }
///      目标 v2: { "amount": 100, "currency": "CNY" }
pub struct CreditedV1ToV2;

impl Upcaster for CreditedV1ToV2 {
    type Event = EventEnvelope;

    fn applies(&self, e: &EventEnvelope) -> bool {
        e.event_type == "account.credited" && e.schema_version == 1
    }

    fn upcast(&self, mut e: EventEnvelope) -> Result<UpcastResult<EventEnvelope>> {
        let amount = e
            .payload
            .get("amount")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("v1 missing amount"))?;

        // 写回新的 payload
        e.payload = json!({
            "amount": amount,
            "currency": "CNY" // 默认值或从 metadata 推断
        });
        e.schema_version = 2;

        Ok(UpcastResult::One(e))
    }
}

pub struct CreditedV2ToV3;

impl Upcaster for CreditedV2ToV3 {
    type Event = EventEnvelope;

    fn applies(&self, e: &EventEnvelope) -> bool {
        e.event_type == "account.credited" && e.schema_version == 2
    }

    fn upcast(&self, mut e: EventEnvelope) -> Result<UpcastResult<EventEnvelope>> {
        let amount = e
            .payload
            .get("amount")
            .and_then(Value::as_i64)
            .ok_or_else(|| anyhow::anyhow!("v2 missing amount as i64"))?;

        // 例：历史是元，这里转成分
        let minor_units = amount * 100;

        let currency = e.payload.get("currency").cloned().unwrap_or(json!("CNY"));
        e.payload = json!({
            "minor_units": minor_units,
            "currency": currency
        });
        e.schema_version = 3;

        Ok(UpcastResult::One(e))
    }
}

pub struct CreditedV3ToSplitV4;

impl Upcaster for CreditedV3ToSplitV4 {
    type Event = EventEnvelope;

    fn applies(&self, e: &EventEnvelope) -> bool {
        e.event_type == "account.credited" && e.schema_version == 3
    }

    fn upcast(&self, e: EventEnvelope) -> Result<UpcastResult<EventEnvelope>> {
        let minor_units = e
            .payload
            .get("minor_units")
            .and_then(Value::as_i64)
            .ok_or_else(|| anyhow::anyhow!("v3 missing minor_units"))?;
        let currency = e.payload.get("currency").cloned().unwrap_or(json!("CNY"));

        let mut funds = e.clone();
        funds.event_type = "funds.received".to_string();
        funds.schema_version = 4;
        funds.payload = json!({ "minor_units": minor_units, "currency": currency });

        let mut ledger = e;
        ledger.event_type = "ledger.posted".to_string();
        ledger.schema_version = 4;
        ledger.payload = json!({ "delta": minor_units, "currency": currency });

        Ok(UpcastResult::Many(vec![funds, ledger]))
    }
}

pub fn default_upcasters() -> UpcasterChain<EventEnvelope> {
    UpcasterChain::new()
        .add(CreditedV1ToV2)
        .add(CreditedV2ToV3)
        .add(CreditedV3ToSplitV4)
}

/// 读取一批原始事件 → 统一升级 → 再按“当前版强类型”反序列化
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DomainEvent {
    #[serde(rename = "funds.received")]
    FundsReceivedV4 { minor_units: i64, currency: String },

    #[serde(rename = "ledger.posted")]
    LedgerPostedV4 { delta: i64, currency: String },
}

pub fn deserialize_current(e: &EventEnvelope) -> Result<DomainEvent> {
    // 你也可以再按 event_type 路由到具体结构
    let t = e.event_type.as_str();
    match t {
        "funds.received" => Ok(serde_json::from_value::<DomainEvent>(json!({
            "type": "funds.received",
            "minor_units": e.payload["minor_units"],
            "currency": e.payload["currency"],
        }))?),
        "ledger.posted" => Ok(serde_json::from_value::<DomainEvent>(json!({
            "type": "ledger.posted",
            "delta": e.payload["delta"],
            "currency": e.payload["currency"],
        }))?),
        _ => anyhow::bail!("unknown event_type after upcast: {}", t),
    }
}

fn main() -> Result<()> {
    // 模拟从事件存储读取的原始事件
    let raw_events = vec![
        EventEnvelope {
            event_type: "account.credited".to_string(),
            schema_version: 1,
            aggregate_id: "acc-123".to_string(),
            aggregate_version: 1,
            global_seq: 1001,
            payload: json!({ "amount": 100 }),
            metadata: json!({ "correlation_id": "corr-1" }),
            timestamp: 1697059200,
        },
        EventEnvelope {
            event_type: "account.credited".to_string(),
            schema_version: 2,
            aggregate_id: "acc-123".to_string(),
            aggregate_version: 2,
            global_seq: 1002,
            payload: json!({ "amount": 200, "currency": "USD" }),
            metadata: json!({ "correlation_id": "corr-2" }),
            timestamp: 1697059260,
        },
        EventEnvelope {
            event_type: "account.credited".to_string(),
            schema_version: 3,
            aggregate_id: "acc-123".to_string(),
            aggregate_version: 3,
            global_seq: 1003,
            payload: json!({ "minor_units": 30000, "currency": "EUR" }),
            metadata: json!({ "correlation_id": "corr-3" }),
            timestamp: 1697059320,
        },
    ];

    let upcaster_chain = default_upcasters();
    let upcasted = upcaster_chain.upcast_all(raw_events)?;

    println!("Upcasted Events: {:#?}", upcasted);

    Ok(())
}
