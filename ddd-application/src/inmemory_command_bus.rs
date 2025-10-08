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
                        Err(_) => Err(AppError::TypeMismatch {
                            expected: C::NAME,
                            found: "unknown",
                        }),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::command_handler::CommandHandler;
    use crate::error::AppError;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::task::JoinSet;

    #[derive(Debug)]
    struct Add;
    impl Command for Add {
        const NAME: &'static str = "Add";
    }

    struct AddHandler {
        counter: Arc<AtomicUsize>,
    }
    #[async_trait]
    impl CommandHandler<Add> for AddHandler {
        async fn handle(&self, _ctx: &AppContext, _cmd: Add) -> Result<(), AppError> {
            self.counter.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn register_and_dispatch_works() {
        let bus = InMemoryCommandBus::new();
        let counter = Arc::new(AtomicUsize::new(0));
        bus.register::<Add, _>(Arc::new(AddHandler {
            counter: counter.clone(),
        }));

        let ctx = AppContext::default();
        bus.dispatch(&ctx, Add).await.unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn not_found_error_when_unregistered() {
        let bus = InMemoryCommandBus::new();
        let ctx = AppContext::default();
        let err = bus.dispatch(&ctx, Add).await.unwrap_err();
        match err {
            AppError::NotFound(name) => assert_eq!(name, Add::NAME),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[derive(Debug)]
    struct Wrong;
    impl Command for Wrong {
        const NAME: &'static str = "Wrong";
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn type_mismatch_error_when_corrupted_entry() {
        let bus = InMemoryCommandBus::new();
        // 手动插入一个错误的条目：键是 Add，但闭包尝试将命令 downcast 为 Wrong
        let f: CmdHandlerFn = Arc::new(|boxed_cmd, _ctx| {
            Box::pin(async move {
                let _ = boxed_cmd
                    .downcast::<Wrong>()
                    .map_err(|_| AppError::TypeMismatch {
                        expected: Wrong::NAME,
                        found: "unknown",
                    })?;
                Ok(())
            })
        });
        bus.handlers.insert(TypeId::of::<Add>(), f);

        let ctx = AppContext::default();
        let err = bus.dispatch(&ctx, Add).await.unwrap_err();
        match err {
            AppError::TypeMismatch { expected, .. } => assert_eq!(expected, Wrong::NAME),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_dispatch_is_safe() {
        let bus = Arc::new(InMemoryCommandBus::new());
        let counter = Arc::new(AtomicUsize::new(0));
        bus.register::<Add, _>(Arc::new(AddHandler {
            counter: counter.clone(),
        }));

        let mut set = JoinSet::new();
        let ctx = AppContext::default();
        for _ in 0..100 {
            let bus = bus.clone();
            let ctx = ctx.clone();
            set.spawn(async move { bus.dispatch(&ctx, Add).await });
        }

        while let Some(res) = set.join_next().await {
            res.unwrap().unwrap();
        }

        assert_eq!(counter.load(Ordering::SeqCst), 100);
    }
}
