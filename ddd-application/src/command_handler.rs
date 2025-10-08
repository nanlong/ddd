use crate::{command::Command, context::AppContext, error::AppError};
use async_trait::async_trait;

#[async_trait]
pub trait CommandHandler<C>: Send + Sync
where
    C: Command,
{
    async fn handle(&self, ctx: &AppContext, cmd: C) -> Result<(), AppError>;
}
