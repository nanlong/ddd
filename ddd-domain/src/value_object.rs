//! 值对象（Value Object）
//!
//! 无标识、以值相等为准的对象，用于封装不可变的概念性值与校验逻辑。
//!
use std::hash::Hash;

/// 值对象抽象
pub trait ValueObject: Clone + PartialEq + Eq + Hash + Send + Sync {
    /// 业务校验失败时的错误类型
    type Error;

    /// 创建值对象时进行验证
    fn validate(&self) -> Result<(), Self::Error>;
}
