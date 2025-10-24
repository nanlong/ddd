use async_trait::async_trait;
use ddd_application::InMemoryCommandBus;
use ddd_application::command_bus::CommandBus;
use ddd_application::command_handler::CommandHandler;
use ddd_application::context::AppContext;
use ddd_application::error::AppError;
use ddd_domain::domain_event::BusinessContext;
use std::sync::Arc;

#[derive(Debug)]
struct CreateUser {
    name: String,
}

struct CreateUserHandler;

#[async_trait]
impl CommandHandler<CreateUser> for CreateUserHandler {
    async fn handle(&self, _ctx: &AppContext, cmd: CreateUser) -> Result<(), AppError> {
        println!("CreateUser: name={}", cmd.name);
        Ok(())
    }
}

#[derive(Debug)]
struct DeleteUser {
    id: u32,
}

struct DeleteUserHandler;

#[async_trait]
impl CommandHandler<DeleteUser> for DeleteUserHandler {
    async fn handle(&self, _ctx: &AppContext, cmd: DeleteUser) -> Result<(), AppError> {
        println!("DeleteUser: id={}", cmd.id);
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bus = InMemoryCommandBus::new();
    bus.register::<CreateUser, _>(Arc::new(CreateUserHandler));
    bus.register::<DeleteUser, _>(Arc::new(DeleteUserHandler));

    let ctx = AppContext {
        biz: BusinessContext::builder()
            .maybe_correlation_id(Some("cor-1".into()))
            .maybe_causation_id(Some("cau-1".into()))
            .maybe_actor_type(Some("user".into()))
            .maybe_actor_id(Some("u-1".into()))
            .build(),
        idempotency_key: Some("idem-1".into()),
    };
    bus.dispatch(
        &ctx,
        CreateUser {
            name: "Alice".into(),
        },
    )
    .await?;
    bus.dispatch(&ctx, DeleteUser { id: 42 }).await?;

    // 未注册的命令 -> 返回 NotFound 错误
    #[allow(dead_code)]
    #[derive(Debug)]
    struct UpdateUser {
        id: u32,
        name: String,
    }

    if let Err(AppError::NotFound(name)) = bus
        .dispatch(
            &ctx,
            UpdateUser {
                id: 7,
                name: "Eve".into(),
            },
        )
        .await
    {
        eprintln!("NotFound as expected for command: {}", name);
    }
    Ok(())
}
