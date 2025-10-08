use crate::{
    context::AppContext, error::AppError, query::Query, query_bus::QueryBus,
    query_handler::QueryHandler,
};
use async_trait::async_trait;
use dashmap::DashMap;
use std::any::{Any, TypeId, type_name};
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
    handlers: DashMap<TypeId, QueryHandlerFn>,
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
    pub fn register<Q, H>(&self, handler: Arc<H>)
    where
        Q: Query + Send + Sync + 'static,
        H: QueryHandler<Q> + Send + Sync + 'static,
    {
        let key = TypeId::of::<Q>();

        let f: QueryHandlerFn = {
            let handler = handler.clone();

            Arc::new(move |boxed_q, ctx| {
                let handler = handler.clone();

                Box::pin(async move {
                    match boxed_q.downcast::<Q>() {
                        Ok(q) => {
                            let dto = handler.handle(ctx, *q).await?;
                            Ok(Box::new(dto) as BoxAnySend)
                        }
                        Err(_) => Err(AppError::TypeMismatch {
                            expected: Q::NAME,
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
impl QueryBus for InMemoryQueryBus {
    async fn dispatch<Q: Query>(&self, ctx: &AppContext, q: Q) -> Result<Q::Dto, AppError> {
        let Some(f) = self.handlers.get(&TypeId::of::<Q>()).map(|h| h.clone()) else {
            return Err(AppError::NotFound(Q::NAME));
        };

        let out = (f)(Box::new(q), ctx).await?;

        match out.downcast::<Q::Dto>() {
            Ok(dto) => Ok(*dto),
            Err(_) => Err(AppError::TypeMismatch {
                expected: type_name::<Q::Dto>(),
                found: "unknown",
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::Dto;
    use crate::error::AppError;
    use crate::query::Query;
    use crate::query_handler::QueryHandler;
    use serde::Serialize;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::task::JoinSet;

    #[derive(Debug)]
    struct Get;
    #[derive(Debug, Serialize)]
    struct NumDto(pub usize);
    impl Dto for NumDto {}
    impl Query for Get {
        const NAME: &'static str = "Get";
        type Dto = NumDto;
    }

    struct GetHandler {
        counter: Arc<AtomicUsize>,
    }
    #[async_trait]
    impl QueryHandler<Get> for GetHandler {
        async fn handle(&self, _ctx: &AppContext, _q: Get) -> Result<NumDto, AppError> {
            let v = self.counter.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(NumDto(v))
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn register_and_dispatch_works() {
        let bus = InMemoryQueryBus::new();
        let counter = Arc::new(AtomicUsize::new(0));
        bus.register::<Get, _>(Arc::new(GetHandler {
            counter: counter.clone(),
        }));

        let ctx = AppContext::default();
        let NumDto(n) = bus.dispatch(&ctx, Get).await.unwrap();
        assert_eq!(n, 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn not_found_error_when_unregistered() {
        let bus = InMemoryQueryBus::new();
        let ctx = AppContext::default();
        let err = bus.dispatch(&ctx, Get).await.unwrap_err();
        match err {
            AppError::NotFound(name) => assert_eq!(name, Get::NAME),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[derive(Debug, Serialize)]
    struct WrongDto;
    impl Dto for WrongDto {}

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn type_mismatch_error_when_result_downcast_fails() {
        let bus = InMemoryQueryBus::new();
        // 手动插入一个错误的条目：键是 Get，但闭包返回 WrongDto 而非 NumDto
        let f: QueryHandlerFn = Arc::new(|_boxed_q, _ctx| {
            Box::pin(async move { Ok(Box::new(WrongDto) as BoxAnySend) })
        });
        bus.handlers.insert(TypeId::of::<Get>(), f);

        let ctx = AppContext::default();
        let err = bus.dispatch(&ctx, Get).await.unwrap_err();
        match err {
            AppError::TypeMismatch { expected, .. } => assert!(expected.contains("NumDto")),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_dispatch_is_safe() {
        let bus = Arc::new(InMemoryQueryBus::new());
        let counter = Arc::new(AtomicUsize::new(0));
        bus.register::<Get, _>(Arc::new(GetHandler {
            counter: counter.clone(),
        }));

        let mut set = JoinSet::new();
        let ctx = AppContext::default();
        for _ in 0..100 {
            let bus = bus.clone();
            let ctx = ctx.clone();
            set.spawn(async move { bus.dispatch(&ctx, Get).await.map(|NumDto(n)| n) });
        }
        let mut results = Vec::new();
        while let Some(res) = set.join_next().await {
            results.push(res.unwrap().unwrap());
        }
        results.sort_unstable();
        assert_eq!(results.len(), 100);
        assert_eq!(results[0], 1);
        assert_eq!(results[99], 100);
    }
}
