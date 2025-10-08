use std::hash::Hash;

pub trait ValueObject: Clone + PartialEq + Eq + Hash + Send + Sync {
    type Error;

    /// 创建值对象时进行验证
    fn validate(&self) -> Result<(), Self::Error>;
}
