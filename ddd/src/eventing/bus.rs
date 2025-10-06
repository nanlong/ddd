use crate::{error::DomainResult as Result, persist::SerializedEvent};
use async_trait::async_trait;
use futures_core::stream::BoxStream;

/// 事件总线：负责分发事件与订阅事件流
#[async_trait]
pub trait EventBus: Send + Sync {
    async fn publish(&self, event: &SerializedEvent) -> Result<()>;

    async fn publish_batch(&self, events: &[SerializedEvent]) -> Result<()> {
        for event in events {
            self.publish(event).await?;
        }
        Ok(())
    }

    /// 返回一个 'static 生命周期的事件流，便于在 tokio::spawn 中使用
    async fn subscribe(&self) -> BoxStream<'static, Result<SerializedEvent>>;
}
