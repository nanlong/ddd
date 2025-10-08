use async_trait::async_trait;
use ddd_application::InMemoryQueryBus;
use ddd_application::context::AppContext;
use ddd_application::dto::Dto;
use ddd_application::error::AppError;
use ddd_application::query::Query;
use ddd_application::query_bus::QueryBus;
use ddd_application::query_handler::QueryHandler;
use ddd_domain::domain_event::BusinessContext;
use serde::Serialize;
use std::sync::Arc;

#[derive(Debug)]
struct GetUser {
    id: u32,
}

#[derive(Debug, Serialize)]
struct UserDto {
    id: u32,
    name: String,
}

// 由于库内未对所有 Serialize+Send+Sync 类型做 Dto 的 blanket impl，这里手动实现一次
impl Dto for UserDto {}

impl Query for GetUser {
    const NAME: &'static str = "GetUser";
    type Dto = UserDto;
}

struct GetUserHandler;

#[async_trait]
impl QueryHandler<GetUser> for GetUserHandler {
    async fn handle(&self, _ctx: &AppContext, q: GetUser) -> Result<UserDto, AppError> {
        Ok(UserDto {
            id: q.id,
            name: "Alice".into(),
        })
    }
}

#[derive(Debug)]
struct ListUsers;

#[derive(Debug, Serialize)]
struct UsersDto(Vec<UserDto>);

impl Dto for UsersDto {}

struct ListUsersHandler;

#[async_trait]
impl QueryHandler<ListUsers> for ListUsersHandler {
    async fn handle(&self, _ctx: &AppContext, _q: ListUsers) -> Result<UsersDto, AppError> {
        Ok(UsersDto(vec![
            UserDto {
                id: 1,
                name: "Alice".into(),
            },
            UserDto {
                id: 2,
                name: "Bob".into(),
            },
        ]))
    }
}

impl Query for ListUsers {
    const NAME: &'static str = "ListUsers";
    type Dto = UsersDto;
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 使用 InMemoryQueryBus（别名）进行进程内查询分发
    let bus = InMemoryQueryBus::new();
    bus.register::<GetUser, _>(Arc::new(GetUserHandler));
    bus.register::<ListUsers, _>(Arc::new(ListUsersHandler));

    let ctx = AppContext {
        biz: BusinessContext::builder()
            .maybe_correlation_id(Some("cor-2".into()))
            .maybe_causation_id(Some("cau-2".into()))
            .maybe_actor_type(Some("user".into()))
            .maybe_actor_id(Some("u-2".into()))
            .build(),
        idempotency_key: None,
    };
    let dto = bus.dispatch(&ctx, GetUser { id: 1 }).await?;
    println!("GetUser: id={}, name={}", dto.id, dto.name);

    let list = bus.dispatch(&ctx, ListUsers).await?;
    println!("ListUsers: count={}", list.0.len());

    // 未注册的查询 -> 返回 NotFound 错误
    #[derive(Debug)]
    struct GetOrders;

    impl Query for GetOrders {
        const NAME: &'static str = "GetOrders";
        type Dto = UsersDto; // 仅为演示，实际应为自己的 DTO 类型
    }

    if let Err(ddd_application::error::AppError::NotFound(name)) =
        bus.dispatch(&ctx, GetOrders).await
    {
        eprintln!("NotFound as expected for query: {}", name);
    }
    Ok(())
}
