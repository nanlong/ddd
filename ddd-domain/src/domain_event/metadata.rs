use bon::Builder;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 元数据
#[derive(Builder, Default, Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    aggregate_id: String,
    aggregate_type: String,
    occurred_at: DateTime<Utc>,
}

impl Metadata {
    pub fn aggregate_id(&self) -> &str {
        &self.aggregate_id
    }

    pub fn aggregate_type(&self) -> &str {
        &self.aggregate_type
    }

    pub fn occurred_at(&self) -> &DateTime<Utc> {
        &self.occurred_at
    }
}
