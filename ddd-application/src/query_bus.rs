use crate::{context::AppContext, error::AppError, query::Query};
use async_trait::async_trait;

#[async_trait]
pub trait QueryBus: Send + Sync {
    async fn dispatch<Q: Query>(&self, ctx: &AppContext, q: Q) -> Result<Q::Dto, AppError>;
}
