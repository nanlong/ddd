use crate::{context::AppContext, error::AppError, query::Query};
use async_trait::async_trait;

/// 查询处理器（Query Handler）
///
/// - 处理具体类型的查询，返回查询的 DTO；
/// - 建议只读，不修改领域状态，可直接访问读模型或投影。
#[async_trait]
pub trait QueryHandler<Q>: Send + Sync
where
    Q: Query,
{
    /// 处理查询并返回结果 DTO
    async fn handle(&self, ctx: &AppContext, q: Q) -> Result<Q::Dto, AppError>;
}
