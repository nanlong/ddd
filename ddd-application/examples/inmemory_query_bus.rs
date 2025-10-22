use async_trait::async_trait;
use ddd_application::InMemoryQueryBus;
use ddd_application::context::AppContext;
use ddd_application::dto::Dto;
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

// 由于库内未对所有 Serialize+Send+Sync 类型做 Dto 的 blanket impl，这里手动实现一次
impl Dto for UserDto {}

struct GetUserHandler;

#[async_trait]
impl QueryHandler<GetUser, UserDto> for GetUserHandler {
    async fn handle(&self, _ctx: &AppContext, q: GetUser) -> Result<Option<UserDto>, AppError> {
        Ok(Some(UserDto {
            id: q.id,
            name: "Alice".into(),
        }))
    }
}

#[derive(Debug)]
struct ListUsers;

#[derive(Debug, Serialize)]
struct UsersDto(Vec<UserDto>);

impl Dto for UsersDto {}

struct ListUsersHandler;

#[async_trait]
impl QueryHandler<ListUsers, UsersDto> for ListUsersHandler {
    async fn handle(&self, _ctx: &AppContext, _q: ListUsers) -> Result<Option<UsersDto>, AppError> {
        Ok(Some(UsersDto(vec![
            UserDto {
                id: 1,
                name: "Alice".into(),
            },
            UserDto {
                id: 2,
                name: "Bob".into(),
            },
        ])))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bus = InMemoryQueryBus::new();
    bus.register::<GetUser, UserDto, _>(Arc::new(GetUserHandler));
    bus.register::<ListUsers, UsersDto, _>(Arc::new(ListUsersHandler));

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
    if let Some(dto) = dto {
        println!("GetUser: id={}, name={}", dto.id, dto.name);
    } else {
        println!("GetUser: not found");
    }

    let list = bus.dispatch::<ListUsers, UsersDto>(&ctx, ListUsers).await?;
    if let Some(list) = list {
        println!("ListUsers: count={}", list.0.len());
    } else {
        println!("ListUsers: empty");
    }

    // 未注册的查询 -> 返回 NotFound 错误
    #[derive(Debug)]
    struct GetOrders;

    if let Err(ddd_application::error::AppError::NotFound(name)) =
        bus.dispatch::<GetOrders, UsersDto>(&ctx, GetOrders).await
    {
        eprintln!("NotFound as expected for query: {}", name);
    }
    Ok(())
}
