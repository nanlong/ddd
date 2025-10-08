# ddd-domain

领域层（Domain Layer）：承载核心业务模型与规则，独立于应用层与基础设施。

## 核心抽象

- `Entity` / `Aggregate`
  - `Entity`：带标识，提供 `new/id/version`
  - `Aggregate`：聚合根接口（`TYPE`，`Command/Event/Error`，`execute/apply`）
- `DomainEvent`
  - 事件 ID、类型、版本、聚合版本等元信息
- `EventEnvelope<A>` / `AggregateEvents<A>`
  - 携带 `Metadata` 与 `BusinessContext`（correlation/causation/actor_*）
- `persist::*`
  - 事件/快照序列化模型与仓储接口（`EventRepository`、`SnapshotRepository`等）
- `eventing::*`
  - 事件总线、投递器、回收器等抽象

## 过程宏配合

本仓库配套的 `ddd-macros` 能生成常用样板：

- `#[entity]`：为具名字段结构体追加 `id`/`version` 并实现 `Entity`
- `#[event]`：为具名字段变体的枚举追加 `id`/`aggregate_version` 并实现 `DomainEvent`

为了在本 crate 的测试与宏展开中解析到绝对路径 `::ddd_domain::...`，库根导出了自引用别名：

```rust
// src/lib.rs
extern crate self as ddd_domain;
```

## 示例与运行

领域层包含多个示例：事件升级（Upcaster）链、聚合命令/事件应用、快照序列化等。

```bash
cargo build   -p ddd-domain
cargo test    -p ddd-domain
cargo run     -p ddd-domain --example event_upcasting
```

## 与应用层/基础设施的关系

- 领域层不依赖应用层与基础设施；
- 应用层依赖领域层；
- 基础设施（如事件存储、消息总线）应依赖应用层接口与领域模型；

建议在后续 `ddd-infrastructure` 中实现事件/快照仓储与消息总线，领域层保持纯净。

