//! 领域层统一错误定义
//!
//! 聚焦序列化/上抬、事件系统、仓储、命令与状态校验等最小必要集合，
//! 便于在各实现层统一转换为 `DomainError`。
//!
use thiserror::Error;

/// 统一错误类型（基础库最小必要集）
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum DomainError {
    // --- 序列化/事件上抬 ---
    #[error("serialization error: {source}")]
    Serde {
        #[from]
        source: serde_json::Error,
    },
    #[error("parse error: {reason}")]
    Parse { reason: String },
    #[error(
        "upcast failed: type={event_type}, from_version={from_version}, stage={stage:?}, reason={reason}"
    )]
    UpcastFailed {
        event_type: String,
        from_version: usize,
        stage: Option<&'static str>,
        reason: String,
    },
    #[error("type mismatch: expected={expected}, found={found}")]
    TypeMismatch { expected: String, found: String },

    // --- 事件系统 ---
    #[error("event bus error: {reason}")]
    EventBus { reason: String },
    #[error("event handler error: handler={handler}, reason={reason}")]
    EventHandler { handler: String, reason: String },

    // --- 仓储/持久化 ---
    #[error("event repository error: {reason}")]
    EventRepository { reason: String },
    #[error("snapshot repository error: {reason}")]
    SnapshotRepository { reason: String },
    #[error("repository error: {reason}")]
    Repository { reason: String },
    #[error("database error: {reason}")]
    Database { reason: String },
    #[error("version conflict: expected={expected}, actual={actual}")]
    VersionConflict { expected: usize, actual: usize },

    // --- 领域规则/命令与状态 ---
    #[error("invalid command: {reason}")]
    InvalidCommand { reason: String },
    #[error("invalid state: {reason}")]
    InvalidState { reason: String },
    #[error("invalid value: {reason}")]
    InvalidValue { reason: String },
    #[error("not found: {reason}")]
    NotFound { reason: String },

    // --- 通用 ---
    #[error("invalid aggregate id: {0}")]
    InvalidAggregateId(String),
}

/// 统一 Result 类型别名
pub type DomainResult<T> = Result<T, DomainError>;

// ---- Cross-crate conversions for infrastructure convenience ----
// 允许在基础设施层直接使用 `?` 将 sqlx/uuid 等错误转换为 DomainError

impl From<sqlx::Error> for DomainError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => DomainError::NotFound {
                reason: "row not found".to_string(),
            },
            other => DomainError::Database {
                reason: other.to_string(),
            },
        }
    }
}

impl From<uuid::Error> for DomainError {
    fn from(err: uuid::Error) -> Self {
        DomainError::Parse {
            reason: err.to_string(),
        }
    }
}

impl From<std::num::ParseIntError> for DomainError {
    fn from(err: std::num::ParseIntError) -> Self {
        DomainError::Parse {
            reason: err.to_string(),
        }
    }
}

impl From<std::num::ParseFloatError> for DomainError {
    fn from(err: std::num::ParseFloatError) -> Self {
        DomainError::Parse {
            reason: err.to_string(),
        }
    }
}

impl From<std::str::ParseBoolError> for DomainError {
    fn from(err: std::str::ParseBoolError) -> Self {
        DomainError::Parse {
            reason: err.to_string(),
        }
    }
}

impl From<chrono::ParseError> for DomainError {
    fn from(err: chrono::ParseError) -> Self {
        DomainError::Parse {
            reason: err.to_string(),
        }
    }
}
