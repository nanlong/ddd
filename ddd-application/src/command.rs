/// 应用层命令（Command）
///
/// 表达“意图”的写操作请求，通常会修改领域状态。
/// - 不返回业务数据，仅表达执行结果（成功/失败）。
/// - 与 [`Query`](crate::query::Query) 相对，`Command` 应避免读写混用。
/// - 建议保持语义化的“动宾结构”命名，如 `CreateUser`、`CloseOrder`。
///
/// 关联常量：
/// - `NAME`：命令的稳定名称，用于日志、追踪与路由。避免依赖 `type_name::<T>()`。
pub trait Command: Send + Sync + 'static {
    /// 命令的稳定名称（建议常量字符串，不随重构变化）
    const NAME: &'static str;
}
