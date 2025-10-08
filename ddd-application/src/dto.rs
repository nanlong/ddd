use serde::Serialize;

/// 数据传输对象（DTO）
///
/// - 作为应用层的输出载体，面向接口/外部系统序列化友好；
/// - 与领域模型解耦，避免将领域对象直接暴露到接口层；
/// - 应保持只读特性与简洁结构，适配不同用例的返回需求。
pub trait Dto: Serialize + Send + Sync + 'static {}
