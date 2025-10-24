use serde::{Deserialize, Serialize};

/// 字段变更封装，包含旧值与新值
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldChanged<T> {
    pub old: T,
    pub new: T,
}

impl<T> FieldChanged<T> {
    pub fn new(old: T, new: T) -> Self {
        Self { old, new }
    }

    pub fn new_value(&self) -> &T {
        &self.new
    }

    pub fn old_value(&self) -> &T {
        &self.old
    }
}

impl<T> FieldChanged<T>
where
    T: PartialEq,
{
    pub fn is_changed(&self) -> bool {
        self.old != self.new
    }
}
