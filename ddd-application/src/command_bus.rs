use crate::{command::Command, context::AppContext, error::AppError};
use async_trait::async_trait;

#[async_trait]
pub trait CommandBus: Send + Sync {
    async fn dispatch<C: Command>(&self, ctx: &AppContext, cmd: C) -> Result<(), AppError>;
}
