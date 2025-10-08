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

/// 进程内（非分布式）的 QueryBus 实现
/// - 通过 TypeId 注册不同 Query 对应的 Handler
/// - 以类型擦除方式调度，并在调用端进行结果还原
pub struct InProcessQueryBus {
    handlers: DashMap<TypeId, QueryHandlerFn>,
}

impl Default for InProcessQueryBus {
    fn default() -> Self {
        Self {
            handlers: DashMap::new(),
        }
    }
}

impl InProcessQueryBus {
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
                            expected: type_name::<Q>(),
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
impl QueryBus for InProcessQueryBus {
    async fn dispatch<Q: Query>(&self, ctx: &AppContext, q: Q) -> Result<Q::Dto, AppError> {
        let Some(f) = self.handlers.get(&TypeId::of::<Q>()).map(|h| h.clone()) else {
            return Err(AppError::NotFound(type_name::<Q>()));
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
