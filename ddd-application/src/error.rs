use ddd_domain::error::DomainError;

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
    NotFound(&'static str),

    #[error("type mismatch: expected={expected}, found={found}")]
    TypeMismatch {
        expected: &'static str,
        found: &'static str,
    },
}
