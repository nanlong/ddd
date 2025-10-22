# DDD 领域驱动设计基础库（Rust）

一个面向 DDD 的轻量级工作区，聚焦“过程宏 → 领域层 → 应用层”的清晰分层与组合方式。

- `ddd-macros`：过程宏，生成实体/实体ID/领域事件样板（减少重复，统一约定）。
- `ddd-domain`：领域层，聚合/事件/上抬链/仓储与事件引擎等抽象与通用实现。
- `ddd-application`：应用层，命令/查询、处理器与总线（内存实现），DTO 与上下文。

## 目录结构

```
.
├── Cargo.toml                # Workspace
├── ddd-macros/               # 过程宏
├── ddd-domain/               # 领域层
└── ddd-application/          # 应用层
```

## 快速开始

环境要求：

- Rust 1.80+（建议 stable，workspace 采用 2024 edition）

构建与测试：

```bash
cargo build
cargo test
```

运行示例：

```bash
# 领域层示例
cargo run -p ddd-domain --example event_upcasting
cargo run -p ddd-domain --example event_repository
cargo run -p ddd-domain --example snapshot_repository
cargo run -p ddd-domain --example eventing_inmemory

# 应用层示例
cargo run -p ddd-application --example inmemory_command_bus
cargo run -p ddd-application --example inmemory_query_bus
```

---

## 1) 过程宏：`ddd-macros`

目标：去样板化，统一实体/实体ID/事件的结构与派生；宏展开时使用绝对路径 `::ddd_domain::...`。

依赖要求：在目标 crate 的 `Cargo.toml` 中引入 serde（用于自动派生）。

```toml
serde = { version = "1", features = ["derive"] }
```

支持宏：

- `#[entity(id = IdType)]`：具名字段结构体 → 追加 `id: IdType`、`version: usize`，实现 `Entity`。
- `#[entity_id]`：单字段 tuple struct → 自动派生 + `FromStr`/`Display`/`AsRef` 等便捷实现。
- `#[event(id = IdType, version = N)]`：具名字段枚举变体 → 追加 `id`/`aggregate_version` 字段并实现 `DomainEvent`；
  - 变体级覆写：`#[event(event_type = "...", event_version = N)]`（不再支持旧的 `#[event_type]`/`#[event_version]`）。

示例：

```rust
use ddd_macros::{entity, entity_id, event};

#[entity_id]
struct UserId(String);

#[entity(id = UserId)]
#[derive(Clone, Default)]
struct UserProfile {
    nickname: String,
}

#[event(version = 1)]
#[derive(Clone, PartialEq)]
enum UserEvent {
    #[event(event_type = "user.created")]
    Created { name: String },
    #[event(event_type = "user.renamed", event_version = 2)]
    Renamed { new_name: String },
}
```

宏特性：

- 自动合并并追加常用派生，避免重复书写；默认派生如下：
  - `#[entity]`：`Debug`, `Default`, `serde::Serialize`, `serde::Deserialize`
  - `#[entity_id]`：`Default`, `Clone`, `Debug`, `serde::Serialize`, `serde::Deserialize`, `PartialEq`, `Eq`, `Hash`
  - `#[event]`：`Debug`, `Clone`, `PartialEq`, `serde::Serialize`, `serde::Deserialize`
- `#[entity]` 会将 `id`/`version` 放在结构体字段最前，并生成 `new/id/version` 实现。
- `#[event]` 会为每个变体补全 `id`/`aggregate_version`，并实现 `DomainEvent` 的访问器方法。

UI 测试：`cargo test -p ddd-macros`

---

## 2) 领域层：`ddd-domain`

模块与职责：

- `aggregate` / `entity`：聚合与实体基础抽象（聚合含 `TYPE`、`Command/Event/Error`，以及 `execute/apply`）。
- `domain_event`：`DomainEvent`、`EventEnvelope`、`AggregateEvents` 与 `BusinessContext/Metadata`。
- `event_upcaster`：事件上抬（版本升级）接口与上抬链 `EventUpcasterChain`。
- `persist`：
  - 仓储协议：`EventRepository`、`SnapshotRepository`；
  - 通用实现：`EventSourcedRepo<E>`、`SnapshotPolicyRepo<E,S>`；
  - 策略/装饰器：`SnapshotPolicy`、`SnapshotRepositoryWithPolicy<R>`；
  - 序列化：`SerializedEvent`、`SerializedSnapshot`、`deserialize_events/serialize_events`。
- `eventing`：事件总线/投递/回收与 `EventEngine`（内存示例）。

最小聚合示例（结合宏）：

```rust
use ddd_domain::aggregate::Aggregate;
use ddd_domain::entity::Entity;
use ddd_domain::error::DomainError;
use ddd_macros::{entity, entity_id, event};
use ulid::Ulid;

#[entity_id]
struct AccountId(String);

#[entity(id = AccountId)]
#[derive(Clone, Default)]
struct BankAccount {
    balance: i64,
}

#[derive(Debug)]
enum Command {
    Deposit(i64),
    Withdraw(i64),
}

#[event(version = 1)]
#[derive(Clone, PartialEq)]
enum Evt {
    #[event(event_type = "account.deposited")]
    Deposited { amount: i64 },
    #[event(event_type = "account.withdrawn")]
    Withdrawn { amount: i64 },
}

impl Aggregate for BankAccount {
    const TYPE: &'static str = "bank_account";
    type Command = Command;
    type Event = Evt;
    type Error = DomainError;

    fn execute(&self, cmd: Command) -> Result<Vec<Evt>, DomainError> {
        match cmd {
            Command::Deposit(n) if n > 0 => Ok(vec![Evt::Deposited {
                id: Ulid::new().to_string(),
                aggregate_version: self.version() + 1,
                amount: n,
            }]),
            Command::Withdraw(n) if n > 0 && self.balance >= n => Ok(vec![Evt::Withdrawn {
                id: Ulid::new().to_string(),
                aggregate_version: self.version() + 1,
                amount: n,
            }]),
            _ => Err(DomainError::InvalidCommand {
                reason: "invalid amount or insufficient".into(),
            }),
        }
    }

    fn apply(&mut self, e: &Evt) {
        match e {
            Evt::Deposited {
                aggregate_version,
                amount,
                ..
            } => {
                self.balance += amount;
                self.version = *aggregate_version;
            }
            Evt::Withdrawn {
                aggregate_version,
                amount,
                ..
            } => {
                self.balance -= amount;
                self.version = *aggregate_version;
            }
        }
    }
}
```

组合通用仓储（仅事件溯源或带快照）：

```rust
use async_trait::async_trait;
use ddd_domain::aggregate::Aggregate;
use ddd_domain::aggregate_root::AggregateRoot;
use ddd_domain::domain_event::BusinessContext;
use ddd_domain::event_upcaster::EventUpcasterChain;
use ddd_domain::persist::{AggregateRepository, EventRepository, EventSourcedRepo, SnapshotPolicy, SnapshotPolicyRepo, SnapshotRepository, SnapshotRepositoryWithPolicy, SerializedEvent};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// 极简内存事件仓储（演示）
#[derive(Default, Clone)]
struct InMemEvents {
    store: Arc<Mutex<HashMap<String, Vec<SerializedEvent>>>>,
}

#[async_trait]
impl EventRepository for InMemEvents {
    async fn get_events<A: Aggregate>(
        &self,
        id: &str,
    ) -> ddd_domain::error::DomainResult<Vec<SerializedEvent>> {
        Ok(
            self
                .store
                .lock()
                .unwrap()
                .get(id)
                .cloned()
                .unwrap_or_default(),
        )
    }

    async fn get_last_events<A: Aggregate>(
        &self,
        id: &str,
        ver: usize,
    ) -> ddd_domain::error::DomainResult<Vec<SerializedEvent>> {
        Ok(
            self
                .store
                .lock()
                .unwrap()
                .get(id)
                .map(|xs| {
                    xs.iter()
                        .filter(|e| e.aggregate_version() > ver)
                        .cloned()
                        .collect()
                })
                .unwrap_or_default(),
        )
    }

    async fn save(
        &self,
        events: Vec<SerializedEvent>,
    ) -> ddd_domain::error::DomainResult<()> {
        if events.is_empty() {
            return Ok(());
        }
        let mut m = self.store.lock().unwrap();
        let key = events[0].aggregate_id().to_string();
        m.entry(key).or_default().extend(events);
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let upcasters = Arc::new(EventUpcasterChain::default());
    let events = Arc::new(InMemEvents::default());

    // 仅事件溯源
    let repo_es = Arc::new(EventSourcedRepo::new(events.clone(), upcasters.clone()));
    let root_es = AggregateRoot::<BankAccount, _>::new(repo_es.clone());

    // 事件 + 快照策略（示意：用同一个事件仓储，快照需自行实现 SnapshotRepository）
    let snaps = Arc::new(SnapshotRepositoryWithPolicy::new(
        DummySnapshotRepo,
        SnapshotPolicy::Every(10),
    ));
    let repo_ss = Arc::new(SnapshotPolicyRepo::new(events.clone(), snaps, upcasters));
    let _root_ss = AggregateRoot::<BankAccount, _>::new(repo_ss);

    // 执行命令
    let id = AccountId::new("acc-1".to_string());
    root_es
        .execute(
            &id.to_string(),
            Command::Deposit(100),
            BusinessContext::default(),
        )
        .await?;
    Ok(())
}

// 仅为演示签名，具体实现见 examples/snapshot_repository.rs
struct DummySnapshotRepo;
#[async_trait]
impl SnapshotRepository for DummySnapshotRepo {
    async fn get_snapshot<A: Aggregate>(
        &self,
        _id: &str,
        _ver: Option<usize>,
    ) -> ddd_domain::error::DomainResult<
        Option<ddd_domain::persist::SerializedSnapshot>,
    > {
        Ok(None)
    }

    async fn save<A: Aggregate>(&self, _a: &A) -> ddd_domain::error::DomainResult<()> {
        Ok(())
    }
}
```

（如需落地到具体存储与消息系统，请在基础设施层实现 `EventRepository`/`SnapshotRepository` 与事件总线接口，并在应用层进行装配。）

---

## 3) 应用层：`ddd-application`

职责：编排用例与对外接口（API/CLI/Job），与领域层解耦，返回 DTO。

核心组件：

- `Command`/`Query`：输入契约（具名常量 `NAME` 用于路由/追踪）。
- `CommandHandler`/`QueryHandler`：处理具体类型的命令/查询（多为编排、调用领域仓储/服务）。
- `CommandBus`/`QueryBus`：按类型分发；提供内存实现 `InMemoryCommandBus`/`InMemoryQueryBus`。
- `Dto`：输出对象抽象（序列化友好）。
- `AppContext`：横切上下文（`BusinessContext`、幂等键）。
- `AppError`：统一错误（含 `Domain`、`NotFound`、`TypeMismatch` 等）。

示例（命令）：

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
    async fn handle(&self, _ctx: &AppContext, _cmd: CreateUser) -> Result<(), ddd_application::error::AppError> { Ok(()) }
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

示例（查询）：

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
    async fn handle(&self, _ctx: &AppContext, q: GetUser) -> Result<UserDto, ddd_application::error::AppError> {
        Ok(UserDto { id: q.id, name: "Alice".into() })
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

与领域层集成（编排示例）：

```rust
use async_trait::async_trait;
use ddd_application::{command::Command, command_handler::CommandHandler, context::AppContext, error::AppError};
use ddd_domain::{aggregate::Aggregate, aggregate_root::AggregateRoot, domain_event::BusinessContext, persist::AggregateRepository};

// 假设已有 BankAccount 聚合与仓储实现（见上文领域层部分）

#[derive(Debug)]
struct Deposit {
    id: String,
    amount: i64,
}
impl Command for Deposit {
    const NAME: &'static str = "Deposit";
}

struct DepositHandler<R, A>
where
    R: AggregateRepository<A>,
    A: Aggregate<Event = Evt, Error = ddd_domain::error::DomainError>,
{
    root: AggregateRoot<A, R>,
}

#[async_trait]
impl<R, A> CommandHandler<Deposit> for DepositHandler<R, A>
where
    R: AggregateRepository<A> + Send + Sync,
    A: Aggregate<Event = Evt, Error = ddd_domain::error::DomainError> + Send + Sync,
{
    async fn handle(&self, _ctx: &AppContext, cmd: Deposit) -> Result<(), AppError> {
        self.root
            .execute(&cmd.id, Command::Deposit(cmd.amount), BusinessContext::default())
            .await?;
        Ok(())
    }
}
```

并发与错误模型：

- 内存总线基于 `DashMap`，在分发前克隆 `Arc` 闭包避免跨 `await` 持锁；测试覆盖 100 次并发分发。
- `AppError::NotFound(name)`：未注册处理器；`TypeMismatch`：类型还原失败（保护注册被错误覆盖）。

---

如需更完整的端到端示例，请运行各 crate 下的 `examples/`。若希望接入真实数据库与消息系统，建议在独立的基础设施层（如 `ddd-infrastructure`）中实现 `EventRepository`/`SnapshotRepository` 与事件总线，并在应用层通过依赖注入装配。
