use crate::event::DomainEvent;
use async_trait::async_trait;
use std::{error::Error, fmt::Display, str::FromStr};

#[async_trait]
pub trait Aggregate: Send + Sync {
    const TYPE: &'static str;

    type Id: Clone + Display + FromStr + ToString;
    type Command;
    type Event: DomainEvent;
    type Error: Error;

    fn new(aggregate_id: Self::Id) -> Self;

    fn id(&self) -> &Self::Id;

    fn version(&self) -> i64;

    /// 执行命令，返回产生的事件列表
    async fn execute(&self, command: Self::Command) -> Result<Vec<Self::Event>, Self::Error>;

    /// 应用事件，更新聚合状态
    fn apply(&mut self, event: &Self::Event);
}
