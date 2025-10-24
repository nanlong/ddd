use bon::Builder;
use serde::{Deserialize, Serialize};

/// 业务上下文信息
#[derive(Builder, Default, Debug, Clone, Serialize, Deserialize)]
pub struct BusinessContext {
    /// 关联ID
    correlation_id: Option<String>,
    /// 因果ID
    causation_id: Option<String>,
    /// 业务耗时
    duration_ms: Option<u128>,
    /// 触发事件的主体类型（如用户、系统等）
    actor_type: Option<String>,
    /// 触发事件的主体ID
    actor_id: Option<String>,
}

impl BusinessContext {
    pub fn correlation_id(&self) -> Option<&str> {
        self.correlation_id.as_deref()
    }

    pub fn causation_id(&self) -> Option<&str> {
        self.causation_id.as_deref()
    }

    pub fn duration_ms(&self) -> Option<u128> {
        self.duration_ms
    }

    pub fn actor_type(&self) -> Option<&str> {
        self.actor_type.as_deref()
    }

    pub fn actor_id(&self) -> Option<&str> {
        self.actor_id.as_deref()
    }
}
