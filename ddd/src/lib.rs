pub mod aggregate;
pub mod aggregate_root;
pub mod domain_event;
pub mod error;
pub mod event_upcaster;
pub mod eventing;
pub mod persist;
pub mod specification;

// 允许在本 crate 内部通过 ::ddd 进行自引用，
// 以便过程宏在本 crate 的单元测试中也能解析到 ::ddd 路径。
extern crate self as ddd;
