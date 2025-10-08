# ddd-application

应用层（Application Layer）：编排用例、承接接口请求、返回 DTO，不直接承载复杂领域规则。

## 核心组件

- `Command` / `Query`
  - `Command::NAME` / `Query::NAME`：稳定名用于日志、追踪与路由
  - `Query::Dto`：查询的返回数据传输对象
- `Dto`
  - 面向接口序列化友好、与领域模型解耦
  - 当前未提供 blanket impl，请为自定义 DTO 手动实现 `Dto`
- `AppContext`
  - 横切上下文：`BusinessContext`（correlation/causation/actor_*）与 `idempotency_key`
- `CommandHandler` / `QueryHandler`
  - 处理具体类型的命令/查询，建议仅做编排：校验、调用领域服务/仓储等
- `CommandBus` / `QueryBus`
  - 按消息类型路由到对应处理器。当前提供内存实现：`InMemoryCommandBus`、`InMemoryQueryBus`
- `AppError`
  - `Domain`、`Validation`、`Authorization`、`Infra`、`NotFound`、`TypeMismatch`

## 内存总线（InMemory*）

- 基于 `dashmap::DashMap` 并发安全，键为 `TypeId`，值为类型擦除的处理闭包
- `NotFound(name)` 使用 `Command::NAME` / `Query::NAME`，输出简洁
- `TypeMismatch`（极少见）用于保护注册表被错误覆盖时的下转失败

### 命令示例

```rust
use async_trait::async_trait;
use ddd_application::command::Command;
use ddd_application::command_bus::CommandBus;
use ddd_application::command_handler::CommandHandler;
use ddd_application::context::AppContext;
use ddd_application::InMemoryCommandBus;

#[derive(Debug)]
struct CreateUser {
    name: String,
}

impl Command for CreateUser {
    const NAME: &'static str = "CreateUser";
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
    bus.register::<CreateUser, _>(std::sync::Arc::new(CreateUserHandler));

    let _ = bus
        .dispatch(&AppContext::default(), CreateUser { name: "Alice".into() })
        .await;
}
```

### 查询示例

```rust
use async_trait::async_trait;
use ddd_application::context::AppContext;
use ddd_application::dto::Dto;
use ddd_application::query::Query;
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

impl Dto for UserDto {}

impl Query for GetUser {
    const NAME: &'static str = "GetUser";
    type Dto = UserDto;
}

struct GetUserHandler;

#[async_trait]
impl QueryHandler<GetUser> for GetUserHandler {
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
    bus.register::<GetUser, _>(std::sync::Arc::new(GetUserHandler));

    let _ = bus
        .dispatch(&AppContext::default(), GetUser { id: 1 })
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
