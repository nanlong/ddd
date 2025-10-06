use thiserror::Error;

/// 统一错误类型（基础库最小必要集）
#[derive(Debug, Error)]
pub enum DomainError {
    // --- 序列化/事件上抬 ---
    #[error("serialization error: {source}")]
    Serde {
        #[from]
        source: serde_json::Error,
    },
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
    #[error("version conflict: expected={expected}, actual={actual}")]
    VersionConflict { expected: usize, actual: usize },

    // --- 领域规则/命令与状态 ---
    #[error("invalid command: {reason}")]
    InvalidCommand { reason: String },
    #[error("invalid state: {reason}")]
    InvalidState { reason: String },
    #[error("not found: {reason}")]
    NotFound { reason: String },

    // --- 通用 ---
    #[error("invalid aggregate id: {0}")]
    InvalidAggregateId(String),
}

/// 统一 Result 类型别名
pub type DomainResult<T> = Result<T, DomainError>;
