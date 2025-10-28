# ddd-macros

帮助为 `ddd-domain` 生成常用样板代码的过程宏集合：`#[entity]`、`#[entity_id]`、`#[event]`、`#[value_object]`。

> 宏在展开时使用绝对路径 `::ddd_domain::...`。`ddd-domain` 已通过 `extern crate self as ddd_domain;` 暴露自引用别名，确保在其测试/示例中宏可正确解析。

## 默认派生一览

- `#[entity]` 默认追加 `#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]`（可用 `debug = false` 关闭）
- `#[entity_id]` 默认追加 `#[derive(Default, Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash)]`（可用 `debug = false` 关闭）
- `#[event]` 默认追加 `#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]`
- `#[value_object]` 默认追加 `#[derive(Default, Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash)]`（可用 `debug = false` 关闭）

说明：宏会与已有 `derive` 合并并去重（对 `Serialize`/`Deserialize` 统一为 `serde::...` 以避免重复）。

## `#[entity]`

作用于具名字段结构体：

- 若缺失则追加字段：`id: IdType` 与 `version: usize`，并移到字段最前；
- 实现 `::ddd_domain::entity::Entity`（`new/id/version`）。
- 自动合成并合并 `#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]`；用户可追加其它派生（宏会与现有 `derive` 合并并去重）。
  - 需在目标 crate 的 `Cargo.toml` 中以 crate 名 `serde` 引入：`serde = { version = "1", features = ["derive"] }`

语法：

```rust
#[entity(id = IdType)]   // 可选，默认 String
#[entity(debug = false)] // 可选，禁止自动派生 Debug（用户可手写自定义 Debug）
#[entity(debug = true)]  // 可选，显式开启自动派生 Debug（默认即开启）
struct Foo {
    // ...
}
```

限制：仅支持具名字段结构体。

## `#[entity_id]`

作用于单字段 tuple struct（例如 `struct AccountId(String);`）：

- 自动合成并合并 `#[derive(Default, Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash)]`；用户可追加其它派生（宏会与现有 `derive` 合并并去重）。
  - 可通过参数 `debug = false` 禁止自动派生 `Debug`，便于手写自定义 `Debug` 实现；
  - 需在目标 crate 的 `Cargo.toml` 中以 crate 名 `serde` 引入：`serde = { version = "1", features = ["derive"] }`
- 实现 `FromStr`（委托内部类型）与 `Display`；
- 提供构造函数：`impl AccountId { pub fn new(value: Inner) -> Self }`
- 追加便捷转换：
  - `AsRef<Inner>` / `AsMut<Inner>`
  - `From<Wrapper> for Inner`
  - `From<&Wrapper> for Inner`（要求 `Inner: Clone`）
  - `From<Inner> for Wrapper`
  - `From<&Inner> for Wrapper`（要求 `Inner: Clone`）
- 仅支持恰好 1 个字段。

## `#[event]`

作用于具名字段枚举（每个变体为具名字段）：

- 若缺失则为每个变体追加 `id: IdType` 与 `aggregate_version: usize`；
- 自动实现 `::ddd_domain::domain_event::DomainEvent`；
- 事件类型名默认为 `EnumName.Variant`，可在变体级覆盖；
- 事件版本默认取枚举级 `version`，可在变体级覆盖。
- 自动合成并合并 `#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]`；用户可追加其它派生（宏会与现有 `derive` 合并并去重）。
  - 需在目标 crate 的 `Cargo.toml` 中以 crate 名 `serde` 引入：`serde = { version = "1", features = ["derive"] }`

语法：

```rust
#[event(id = IdType, version = 1)]
enum FooEvent {
    // 变体级覆盖：
    #[event(event_type = "FooEvent.Created", event_version = 2)]
    Created {
        id: String,
        aggregate_version: usize,
        name: String
    },
}
```

注意：不再支持旧语法 `#[event_type]` / `#[event_version]` 作为独立属性；请使用统一的 `#[event(event_type = ..., event_version = ...)]` 形式在变体级覆写。

## `#[value_object]`

作用于结构体（具名或 tuple）与枚举：

- 自动合成并合并 `#[derive(Default, Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash)]`；
- 可通过参数 `debug = false` 禁止自动派生 `Debug`，便于手写自定义 `Debug` 实现；
- 不改变已有字段/变体，仅做派生合并；
- 与已有 `derive` 合并并去重；
- 若目标为枚举且启用 `Default`，需在某个变体上标注 `#[default]`（否则编译器会提示缺失默认变体）。

语法：

```rust
#[value_object]
#[value_object(debug = false)]
struct Money {
    amount: i64,
    currency: String,
}

#[value_object]
struct Amount(i64);

#[value_object]
enum Level {
    #[default]
    Low,
    High,
}
```

## 运行 UI 测试

宏 crate 含 `trybuild` UI 测试：

```bash
cargo test -p ddd-macros
```
