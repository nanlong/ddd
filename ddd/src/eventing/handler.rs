use crate::persist::SerializedEvent;
use anyhow::Result;
use async_trait::async_trait;

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
