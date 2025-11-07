use crate::{
    command_bus::CommandBus, command_handler::CommandHandler, context::AppContext, error::AppError,
};
use async_trait::async_trait;
use dashmap::DashMap;
use std::any::{Any, TypeId, type_name, type_name_of_val};
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
    handlers: DashMap<TypeId, (&'static str, CmdHandlerFn)>,
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
    pub fn register<C, H>(&self, handler: Arc<H>) -> Result<(), AppError>
    where
        C: Send + 'static,
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
                        Err(e) => {
                            let found = type_name_of_val(&e);

                            Err(AppError::TypeMismatch {
                                expected: type_name::<C>(),
                                found,
                            })
                        }
                    }
                })
            })
        };

        if self.handlers.contains_key(&key) {
            return Err(AppError::AlreadyRegisteredCommand {
                command: type_name::<C>(),
            });
        }

        self.handlers.insert(key, (type_name::<C>(), f));

        Ok(())
    }
}

#[async_trait]
impl CommandBus for InMemoryCommandBus {
    async fn dispatch<C>(&self, ctx: &AppContext, cmd: C) -> Result<(), AppError>
    where
        C: Send + 'static,
    {
        self.dispatch_impl(ctx, cmd).await
    }
}

impl InMemoryCommandBus {
    async fn dispatch_impl<C>(&self, ctx: &AppContext, cmd: C) -> Result<(), AppError>
    where
        C: Send + 'static,
    {
        let Some((_name, f)) = self.handlers.get(&TypeId::of::<C>()).map(|h| h.clone()) else {
            return Err(AppError::HandlerNotFound(type_name::<C>()));
        };

        (f)(Box::new(cmd), ctx).await
    }
}

impl InMemoryCommandBus {
    /// 获取已注册的命令类型名列表（只读视图）
    pub fn registered_commands(&self) -> Vec<&'static str> {
        self.handlers.iter().map(|e| e.value().0).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command_handler::CommandHandler;
    use crate::error::AppError;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::task::JoinSet;

    #[derive(Debug)]
    struct Add;

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
        }))
        .unwrap();

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
            AppError::HandlerNotFound(name) => assert!(name.contains("Add")),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[derive(Debug)]
    struct Wrong;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn type_mismatch_error_when_corrupted_entry() {
        let bus = InMemoryCommandBus::new();
        // 手动插入一个错误的条目：键是 Add，但闭包尝试将命令 downcast 为 Wrong
        let f: CmdHandlerFn = Arc::new(|boxed_cmd, _ctx| {
            Box::pin(async move {
                let found = type_name_of_val(&boxed_cmd);
                let _ = boxed_cmd
                    .downcast::<Wrong>()
                    .map_err(|_| AppError::TypeMismatch {
                        expected: type_name::<Wrong>(),
                        found,
                    })?;
                Ok(())
            })
        });
        bus.handlers
            .insert(TypeId::of::<Add>(), (type_name::<Add>(), f));

        let ctx = AppContext::default();
        let err = bus.dispatch(&ctx, Add).await.unwrap_err();
        match err {
            AppError::TypeMismatch { expected, .. } => assert!(expected.contains("Wrong")),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_dispatch_is_safe() {
        let bus = Arc::new(InMemoryCommandBus::new());
        let counter = Arc::new(AtomicUsize::new(0));
        bus.register::<Add, _>(Arc::new(AddHandler {
            counter: counter.clone(),
        }))
        .unwrap();

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
