use async_trait::async_trait;

/// 领域服务：封装不属于单个聚合的领域逻辑
#[async_trait]
pub trait DomainService: Send + Sync {
    type Input;
    type Output;
    type Error;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error>;
}
