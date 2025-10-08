use crate::{
    command::Command, command_bus::CommandBus, command_handler::CommandHandler,
    context::AppContext, error::AppError,
};
use async_trait::async_trait;
use dashmap::DashMap;
use std::any::{Any, TypeId};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

type CmdHandlerFuture<'a> = Pin<Box<dyn Future<Output = Result<(), AppError>> + Send + 'a>>;

type CmdHandlerFn =
    Arc<dyn for<'a> Fn(Box<dyn Any + Send>, &'a AppContext) -> CmdHandlerFuture<'a> + Send + Sync>;

/// 基于内存的 CommandBus 实现
/// - 通过 TypeId 注册不同 Command 对应的 Handler
/// - 运行时以类型擦除（Any）方式进行调度
pub struct InMemoryCommandBus {
    handlers: DashMap<TypeId, CmdHandlerFn>,
}

impl Default for InMemoryCommandBus {
    fn default() -> Self {
        Self {
            handlers: DashMap::new(),
        }
    }
}

impl InMemoryCommandBus {
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册命令处理器
    pub fn register<C, H>(&self, handler: Arc<H>)
    where
        C: Command + Send + Sync + 'static,
        H: CommandHandler<C> + Send + Sync + 'static,
    {
        let key = TypeId::of::<C>();

        let f: CmdHandlerFn = {
            let handler = handler.clone();

            Arc::new(move |boxed_cmd, ctx| {
                let handler = handler.clone();

                Box::pin(async move {
                    // 正常情况下这里的 downcast 永远不会失败（键与闭包同一泛型 C）
                    match boxed_cmd.downcast::<C>() {
                        Ok(cmd) => handler.handle(ctx, *cmd).await,
                        Err(_) => Err(AppError::TypeMismatch { expected: C::NAME, found: "unknown" }),
                    }
                })
            })
        };

        self.handlers.insert(key, f);
    }
}

#[async_trait]
impl CommandBus for InMemoryCommandBus {
    async fn dispatch<C: Command>(&self, ctx: &AppContext, cmd: C) -> Result<(), AppError> {
        let Some(f) = self.handlers.get(&TypeId::of::<C>()).map(|h| h.clone()) else {
            return Err(AppError::NotFound(C::NAME));
        };

        (f)(Box::new(cmd), ctx).await
    }
}
