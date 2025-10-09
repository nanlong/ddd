//! 事件处理器（EventHandler）
//!
//! 定义消费某类/多类/全部事件的处理逻辑与元信息（名称、订阅类型）。
//!
use crate::{error::DomainResult as Result, persist::SerializedEvent};
use async_trait::async_trait;

#[derive(Clone, Debug)]
pub enum HandledEventType {
    One(String),
    Many(Vec<String>),
    All,
}

/// 事件处理器：处理某一类型的事件
#[async_trait]
pub trait EventHandler: Send + Sync {
    async fn handle(&self, event: &SerializedEvent) -> Result<()>;

    /// 返回该处理器支持的事件类型
    fn handled_event_type(&self) -> HandledEventType;

    /// 处理器名称（用于失败标记与审计）
    fn handler_name(&self) -> &str;
}
