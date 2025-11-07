use ddd_domain::error::DomainError;

#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("domain: {0}")]
    Domain(#[from] DomainError),

    #[error("validation: {0}")]
    Validation(String),

    #[error("authorization: {0}")]
    Authorization(String),

    #[error("infra: {0}")]
    Infra(String),

    #[error("handler not found: {0}")]
    HandlerNotFound(&'static str),

    #[error("aggregate not found: {0}")]
    AggregateNotFound(String),

    #[error("handler already registered: command={command}")]
    AlreadyRegisteredCommand { command: &'static str },

    #[error("handler already registered: query={query}, result={result}")]
    AlreadyRegisteredQuery {
        query: &'static str,
        result: &'static str,
    },

    #[error("type mismatch: expected={expected}, found={found}")]
    TypeMismatch {
        expected: &'static str,
        found: &'static str,
    },
}
