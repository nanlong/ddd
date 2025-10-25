use crate::{
    context::AppContext, error::AppError, query_bus::QueryBus, query_handler::QueryHandler,
};
use async_trait::async_trait;
use dashmap::DashMap;
use std::any::{Any, TypeId, type_name, type_name_of_val};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

type BoxAnySend = Box<dyn Any + Send>;

type QueryHandlerFuture<'a> =
    Pin<Box<dyn Future<Output = Result<BoxAnySend, AppError>> + Send + 'a>>;

type QueryHandlerFn =
    Arc<dyn for<'a> Fn(BoxAnySend, &'a AppContext) -> QueryHandlerFuture<'a> + Send + Sync>;

/// 基于内存的 QueryBus 实现
/// - 通过 TypeId 注册不同 Query 对应的 Handler
/// - 以类型擦除方式调度，并在调用端进行结果还原
pub struct InMemoryQueryBus {
    handlers: DashMap<TypeId, (&'static str, QueryHandlerFn)>,
}

impl Default for InMemoryQueryBus {
    fn default() -> Self {
        Self {
            handlers: DashMap::new(),
        }
    }
}

impl InMemoryQueryBus {
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册查询处理器
    pub fn register<Q, R, H>(&self, handler: Arc<H>) -> Result<(), AppError>
    where
        Q: Send + 'static,
        R: Send + 'static,
        H: QueryHandler<Q, R> + Send + Sync + 'static,
    {
        let key = TypeId::of::<Q>();

        let f: QueryHandlerFn = {
            let handler = handler.clone();

            Arc::new(move |boxed_q, ctx| {
                let handler = handler.clone();

                Box::pin(async move {
                    match boxed_q.downcast::<Q>() {
                        Ok(q) => {
                            let dto_opt = handler.handle(ctx, *q).await?;
                            Ok(Box::new(dto_opt) as BoxAnySend)
                        }
                        Err(e) => Err(AppError::TypeMismatch {
                            expected: type_name::<Q>(),
                            found: type_name_of_val(&e),
                        }),
                    }
                })
            })
        };

        if self.handlers.contains_key(&key) {
            return Err(AppError::AlreadyRegistered(type_name::<Q>()));
        }

        self.handlers.insert(key, (type_name::<Q>(), f));

        Ok(())
    }
}

#[async_trait]
impl QueryBus for InMemoryQueryBus {
    async fn dispatch<Q, R>(&self, ctx: &AppContext, q: Q) -> Result<R, AppError>
    where
        Q: Send + 'static,
        R: Send + 'static,
    {
        self.dispatch_impl::<Q, R>(ctx, q).await
    }
}

impl InMemoryQueryBus {
    async fn dispatch_impl<Q, R>(&self, ctx: &AppContext, q: Q) -> Result<R, AppError>
    where
        Q: Send + 'static,
        R: Send + 'static,
    {
        let Some((_name, f)) = self.handlers.get(&TypeId::of::<Q>()).map(|h| h.clone()) else {
            return Err(AppError::HandlerNotFound(type_name::<Q>()));
        };

        let out = (f)(Box::new(q), ctx).await?;

        match out.downcast::<R>() {
            Ok(dto_opt) => Ok(*dto_opt),
            Err(e) => Err(AppError::TypeMismatch {
                expected: type_name::<R>(),
                found: type_name_of_val(&e),
            }),
        }
    }
}

impl InMemoryQueryBus {
    /// 获取已注册的查询类型名列表（只读视图）
    pub fn registered_queries(&self) -> Vec<&'static str> {
        self.handlers.iter().map(|e| e.value().0).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AppError;
    use crate::query_handler::QueryHandler;
    use serde::Serialize;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::task::JoinSet;

    #[derive(Debug)]
    struct Get;

    #[derive(Debug, Serialize)]
    struct NumDto(pub usize);

    struct GetHandler {
        counter: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl QueryHandler<Get, NumDto> for GetHandler {
        async fn handle(&self, _ctx: &AppContext, _q: Get) -> Result<NumDto, AppError> {
            let v = self.counter.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(NumDto(v))
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn register_and_dispatch_works() {
        let bus = InMemoryQueryBus::new();
        let counter = Arc::new(AtomicUsize::new(0));
        bus.register::<Get, NumDto, _>(Arc::new(GetHandler {
            counter: counter.clone(),
        }))
        .unwrap();

        let ctx = AppContext::default();
        let NumDto(n) = bus.dispatch::<Get, NumDto>(&ctx, Get).await.unwrap();
        assert_eq!(n, 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn not_found_error_when_unregistered() {
        let bus = InMemoryQueryBus::new();
        let ctx = AppContext::default();
        let err = bus.dispatch::<Get, NumDto>(&ctx, Get).await.unwrap_err();
        match err {
            AppError::HandlerNotFound(name) => assert!(name.contains("Get")),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[derive(Debug, Serialize)]
    struct WrongDto;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn type_mismatch_error_when_result_downcast_fails() {
        let bus = InMemoryQueryBus::new();
        // 手动插入一个错误的条目：键是 Get，但闭包返回 WrongDto 而非 NumDto
        let f: QueryHandlerFn = Arc::new(|_boxed_q, _ctx| {
            Box::pin(async move { Ok(Box::new(WrongDto) as BoxAnySend) })
        });
        bus.handlers
            .insert(TypeId::of::<Get>(), (type_name::<Get>(), f));

        let ctx = AppContext::default();
        let err = bus.dispatch::<Get, NumDto>(&ctx, Get).await.unwrap_err();
        match err {
            AppError::TypeMismatch { expected, .. } => assert!(expected.contains("NumDto")),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_dispatch_is_safe() {
        let bus = Arc::new(InMemoryQueryBus::new());
        let counter = Arc::new(AtomicUsize::new(0));
        bus.register::<Get, NumDto, _>(Arc::new(GetHandler {
            counter: counter.clone(),
        }))
        .unwrap();

        let mut set = JoinSet::new();
        let ctx = AppContext::default();
        for _ in 0..100 {
            let bus = bus.clone();
            let ctx = ctx.clone();
            set.spawn(async move { bus.dispatch::<Get, NumDto>(&ctx, Get).await.unwrap() });
        }
        let mut results = Vec::new();
        while let Some(res) = set.join_next().await {
            results.push(res.unwrap().0);
        }
        results.sort_unstable();
        assert_eq!(results.len(), 100);
        assert_eq!(results[0], 1);
        assert_eq!(results[99], 100);
    }
}
