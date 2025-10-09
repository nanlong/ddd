//! 实体（Entity）基础抽象
//!
//! 为聚合与实体提供统一的标识（Id）与版本（optimistic locking）能力。
//!
use std::{fmt::Display, str::FromStr};

/// 具备唯一标识与版本的实体抽象
pub trait Entity: Send + Sync {
    /// 实体标识类型，要求可解析、可显示与可克隆
    type Id: FromStr + Clone + Display;

    /// 使用给定标识创建实体（聚合）
    fn new(aggregate_id: Self::Id) -> Self;

    /// 获取实体标识
    fn id(&self) -> &Self::Id;

    /// 获取当前版本（用于乐观锁与并发控制）
    fn version(&self) -> usize;
}
