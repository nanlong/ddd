use crate::domain_event::DomainEvent;
use serde::{Serialize, de::DeserializeOwned};
use std::{error::Error, fmt::Display, str::FromStr};

/// 聚合根接口
pub trait Aggregate: Default + Serialize + DeserializeOwned + Send + Sync {
    const TYPE: &'static str;

    type Id: FromStr + AsRef<str> + Clone + Display;
    type Command;
    type Event: DomainEvent;
    type Error: Error;

    fn new(aggregate_id: Self::Id) -> Self;

    fn id(&self) -> &Self::Id;

    fn version(&self) -> usize;

    /// 执行命令，返回产生的事件列表
    fn execute(&self, command: Self::Command) -> Result<Vec<Self::Event>, Self::Error>;

    /// 应用事件，更新聚合状态
    fn apply(&mut self, event: &Self::Event);
}
