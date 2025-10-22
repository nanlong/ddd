use crate::{context::AppContext, error::AppError};
use async_trait::async_trait;

/// 查询总线（Query Bus）
///
/// - 负责根据查询的具体类型路由到对应的处理器；
/// - 适用于进程内或跨进程的查询调度；
/// - 对外返回与查询关联的 DTO 类型。
#[async_trait]
pub trait QueryBus: Send + Sync {
    /// 分发查询到对应处理器，返回该查询的 DTO
    async fn dispatch<Q, R>(&self, ctx: &AppContext, q: Q) -> Result<Option<R>, AppError>
    where
        Q: Send + Sync + 'static,
        R: Send + Sync + 'static;
}
