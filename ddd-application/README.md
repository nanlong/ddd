# ddd-application

应用层（Application Layer）：编排用例、承接接口请求，返回结果对象，不直接承载复杂领域规则。

## 核心组件

- 命令/查询：任意 `Send + 'static` 的类型即可，不再需要实现 `Command`/`Query` trait；路由依据 `TypeId`。
- `AppContext`：横切上下文：`BusinessContext`（correlation/causation/actor_*）与 `idempotency_key`。
- `CommandHandler<C>` / `QueryHandler<Q, R>`：处理具体类型的命令/查询；查询返回 `R`（若需要“可能不存在”，可令 `R = Option<T>` 或以领域层 `NotFound` 表达）。
- `CommandBus` / `QueryBus`：按类型分发；当前提供内存实现：`InMemoryCommandBus`、`InMemoryQueryBus`。
- `AppError`：`Domain`、`Validation`、`Authorization`、`Infra`、`HandlerNotFound`、`AlreadyRegistered`、`TypeMismatch`。

## 内存总线（InMemory*）

- 基于 `dashmap::DashMap` 并发安全，键为 `TypeId`，值为类型擦除的处理闭包
- `HandlerNotFound(name)` 使用类型名 `std::any::type_name::<T>()`，输出简洁
- `TypeMismatch`（极少见）用于保护注册表被错误覆盖时的下转失败

### 命令示例

```rust
 use async_trait::async_trait;
 use ddd_application::command_bus::CommandBus;
 use ddd_application::command_handler::CommandHandler;
 use ddd_application::context::AppContext;
 use ddd_application::InMemoryCommandBus;

#[derive(Debug)]
struct CreateUser {
    name: String,
}

 struct CreateUserHandler;

#[async_trait]
impl CommandHandler<CreateUser> for CreateUserHandler {
    async fn handle(
        &self,
        _ctx: &AppContext,
        _cmd: CreateUser,
    ) -> Result<(), ddd_application::error::AppError> {
        Ok(())
    }
}

 #[tokio::main]
 async fn main() {
     let bus = InMemoryCommandBus::new();
     bus.register::<CreateUser, _>(std::sync::Arc::new(CreateUserHandler)).unwrap();

    let _ = bus
        .dispatch(&AppContext::default(), CreateUser { name: "Alice".into() })
        .await;
 }
 ```

### 查询示例

```rust
 use async_trait::async_trait;
 use ddd_application::context::AppContext;
 use ddd_application::query_bus::QueryBus;
 use ddd_application::query_handler::QueryHandler;
 use ddd_application::InMemoryQueryBus;
 use serde::Serialize;

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
     async fn handle(
         &self,
         _ctx: &AppContext,
         q: GetUser,
     ) -> Result<UserDto, ddd_application::error::AppError> {
         Ok(UserDto {
             id: q.id,
             name: "Alice".into(),
         })
     }
 }

 #[tokio::main]
 async fn main() {
     let bus = InMemoryQueryBus::new();
     bus.register::<GetUser, UserDto, _>(std::sync::Arc::new(GetUserHandler)).unwrap();

     let _ = bus
         .dispatch::<GetUser, UserDto>(&AppContext::default(), GetUser { id: 1 })
         .await;
 }
 ```

## 并发与对象安全

- 并发：注册表基于 `DashMap`，分发前克隆 `Arc` 闭包，避免跨 `await` 持锁
- 对象安全：`dispatch` 为泛型方法，trait 不是对象安全；常以具体实现类型（如 `InMemory*`）注入

## 运行与测试

```bash
cargo build -p ddd-application
cargo test  -p ddd-application
 cargo run   -p ddd-application --example inmemory_command_bus
 cargo run   -p ddd-application --example inmemory_query_bus
 ```

 
