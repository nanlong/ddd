use crate::{context::AppContext, error::AppError, query::Query};
use async_trait::async_trait;

#[async_trait]
pub trait QueryHandler<Q>: Send + Sync
where
    Q: Query,
{
    async fn handle(&self, ctx: &AppContext, q: Q) -> Result<Q::Dto, AppError>;
}
