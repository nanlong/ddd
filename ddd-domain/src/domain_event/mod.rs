//! 领域事件（Domain Event）与事件集合
//!
//! 定义事件载荷需要实现的最小接口（`DomainEvent`），以及将事件与元数据/上下文
//! 封装后的 `EventEnvelope` 与辅助集合类型 `AggregateEvents`。

mod aggregate_events;
mod business_context;
mod domain_event_trait;
mod event_envelope;
mod field_changed;
mod metadata;

pub use aggregate_events::AggregateEvents;
pub use business_context::BusinessContext;
pub use domain_event_trait::DomainEvent;
pub use event_envelope::EventEnvelope;
pub use field_changed::FieldChanged;
pub use metadata::Metadata;
