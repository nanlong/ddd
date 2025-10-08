# DDD 领域驱动设计基础库

- `ddd-domain`：领域层（实体、值对象、聚合、领域事件、事件风格持久化接口等）
- `ddd-application`：应用层（Command/Query、Handler、Bus、DTO、上下文与错误模型）
- `ddd-macros`：过程宏（`#[entity]`、`#[entity_id]`、`#[event]`）帮助快速生成样板代码

## 目录结构

```
.
├── Cargo.toml                # workspace 定义
├── ddd-domain/               # 领域层
├── ddd-application/          # 应用层
└── ddd-macros/               # 过程宏
```

## 快速开始

环境要求：

- Rust 1.80+（建议 stable，工作区使用 2024 edition）

## 应用层（`ddd-application`）

应用层负责编排用例与对外接口，不包含复杂的领域规则。核心组件：

- `Command` / `Query`：用例的输入契约
  - 需实现关联常量 `NAME`，用于路由、追踪与日志
  - `Query` 关联返回类型 `type Dto: Dto`
- `Dto`：应用层输出对象（序列化友好、与领域模型解耦）
- `AppContext`：一次调用的横切上下文（链路追踪、执行者、幂等键）
- `CommandHandler` / `QueryHandler`：处理具体类型的命令/查询
- `CommandBus` / `QueryBus`：按类型路由到对应处理器
- `AppError`：应用层错误模型（`Domain`、`Validation`、`Authorization`、`Infra`、`NotFound`、`TypeMismatch`）

内存总线实现：

- `InMemoryCommandBus`、`InMemoryQueryBus` 基于 `dashmap::DashMap` 并发安全
- 以 `TypeId` 为键注册处理器，运行时类型擦除 + 下转型调度

最小示例（命令）：

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

最小示例（查询）：

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

示例更多请见：`ddd-application/examples/`。

### 并发与错误

- 并发：使用 `DashMap` 避免跨 `await` 的锁持有，内存总线在我们的单元测试中支持 100 并发分发
- 错误：
  - `NotFound(name)`：未注册处理器（使用 `Command::NAME` / `Query::NAME`）
  - `TypeMismatch { expected, found }`：类型下转失败（极少发生，通常由错误注册导致）

## 领域层（`ddd-domain`）

提供 DDD 核心抽象与事件风格的持久化契约：

- Trait 与模块：
  - `Entity`、`Aggregate`、`AggregateRoot`
  - `DomainEvent`、`EventEnvelope`、`AggregateEvents`
  - `persist::*`：事件仓储、快照仓储的接口与序列化形式
  - `eventing::*`：事件总线、投递、回收等抽象

内置多个示例，演示：

- 事件升级（Upcasting）链
- 聚合命令/事件应用
- 快照（Snapshot）的序列化与类型校验

运行：

```bash
cargo run -p ddd-domain --example event_upcasting
```

## 过程宏（`ddd-macros`）

帮助消除样板代码（内部使用绝对路径 `::ddd_domain::...`，已在 `ddd-domain` 中提供自引用别名确保测试环境可解析）：

- `#[entity(id = IdType)]`：为具名字段结构体追加 `id`/`version`，并实现 `Entity`
- `#[entity_id]`：为单字段 tuple struct 生成 `FromStr` / `Display`
- `#[event(id = IdType, version = N)]`：为具名字段枚举变体补全 `id`/`aggregate_version` 字段并实现 `DomainEvent`

示例（简化）：

```rust
use ddd_macros::{entity, event};

#[entity]
#[derive(Default, Clone)]
struct User {
    name: String,
}

#[event(version = 1)]
#[derive(Clone, PartialEq)]
enum UserEvent {
    Created {
        id: String,
        aggregate_version: usize,
        name: String,
    },
}
```
