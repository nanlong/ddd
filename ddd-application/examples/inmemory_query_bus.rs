use async_trait::async_trait;
use ddd_application::InMemoryQueryBus;
use ddd_application::context::AppContext;
use ddd_application::error::AppError;
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

struct GetUserHandler;

#[async_trait]
impl QueryHandler<GetUser, UserDto> for GetUserHandler {
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

struct ListUsersHandler;

#[async_trait]
impl QueryHandler<ListUsers, UsersDto> for ListUsersHandler {
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bus = InMemoryQueryBus::new();
    bus.register::<GetUser, UserDto, _>(Arc::new(GetUserHandler))?;
    bus.register::<ListUsers, UsersDto, _>(Arc::new(ListUsersHandler))?;

    let ctx = AppContext {
        biz: BusinessContext::builder()
            .maybe_correlation_id(Some("cor-2".into()))
            .maybe_causation_id(Some("cau-2".into()))
            .maybe_actor_type(Some("user".into()))
            .maybe_actor_id(Some("u-2".into()))
            .build(),
        idempotency_key: None,
    };
    let dto = bus
        .dispatch::<GetUser, UserDto>(&ctx, GetUser { id: 1 })
        .await?;

    println!("GetUser: id={}, name={}", dto.id, dto.name);

    let list = bus.dispatch::<ListUsers, UsersDto>(&ctx, ListUsers).await?;

    println!("ListUsers: count={}", list.0.len());

    // 未注册的查询 -> 返回 HandlerNotFound 错误
    #[derive(Debug)]
    struct GetOrders;

    if let Err(ddd_application::error::AppError::HandlerNotFound(name)) =
        bus.dispatch::<GetOrders, UsersDto>(&ctx, GetOrders).await
    {
        eprintln!("HandlerNotFound as expected for query: {}", name);
    }

    eprintln!("Registered Queries: {:?}", bus.registered_queries());
    Ok(())
}
