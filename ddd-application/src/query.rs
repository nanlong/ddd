use crate::dto::Dto;

/// 应用层查询（Query）
///
/// 表达只读意图，不改变领域状态。
/// - 结果返回 [`Dto`](crate::dto::Dto)；
/// - 与 [`Command`](crate::command::Command) 相对，`Query` 应避免副作用；
/// - 可按 CQRS 将写/读分离，查询可直连读模型或投影存储。
pub trait Query: Send + Sync + 'static {
    /// 查询的稳定名称（建议常量字符串，不随重构变化）
    const NAME: &'static str;

    /// 查询返回的数据传输对象（序列化友好、与领域模型解耦）
    type Dto: Dto;
}
