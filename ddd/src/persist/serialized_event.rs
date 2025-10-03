use crate::{
    aggregate::Aggregate,
    domain_event::{BusinessContext, DomainEvent, EventEnvelope, Metadata},
    event_upcaster::EventUpcasterChain,
};
use anyhow::Result;
use bon::Builder;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Builder, Serialize, Deserialize)]
pub struct SerializedEvent {
    aggregate_id: String,
    aggregate_type: String,
    event_type: String,
    event_version: usize,
    correlation_id: Option<String>,
    causation_id: Option<String>,
    actor_type: Option<String>,
    actor_id: Option<String>,
    occurred_at: DateTime<Utc>,
    payload: Value,
}

impl SerializedEvent {
    pub fn aggregate_id(&self) -> &str {
        &self.aggregate_id
    }

    pub fn aggregate_type(&self) -> &str {
        &self.aggregate_type
    }

    pub fn event_type(&self) -> &str {
        &self.event_type
    }

    pub fn event_version(&self) -> usize {
        self.event_version
    }

    pub fn correlation_id(&self) -> Option<&str> {
        self.correlation_id.as_deref()
    }

    pub fn causation_id(&self) -> Option<&str> {
        self.causation_id.as_deref()
    }

    pub fn actor_type(&self) -> Option<&str> {
        self.actor_type.as_deref()
    }

    pub fn actor_id(&self) -> Option<&str> {
        self.actor_id.as_deref()
    }

    pub fn occurred_at(&self) -> DateTime<Utc> {
        self.occurred_at
    }

    pub fn payload(&self) -> &Value {
        &self.payload
    }
}

impl<A> TryFrom<&EventEnvelope<A>> for SerializedEvent
where
    A: Aggregate,
{
    type Error = serde_json::Error;

    fn try_from(envelope: &EventEnvelope<A>) -> Result<Self, Self::Error> {
        Ok(SerializedEvent {
            aggregate_id: envelope.metadata.aggregate_id().to_string(),
            aggregate_type: envelope.metadata.aggregate_type().to_string(),
            event_type: envelope.payload.event_type(),
            event_version: envelope.payload.event_version(),
            correlation_id: envelope.context.correlation_id().map(|s| s.to_string()),
            causation_id: envelope.context.causation_id().map(|s| s.to_string()),
            actor_type: envelope.context.actor_type().map(|s| s.to_string()),
            actor_id: envelope.context.actor_id().map(|s| s.to_string()),
            occurred_at: envelope.metadata.occurred_at().clone(),
            payload: serde_json::to_value(&envelope.payload)?,
        })
    }
}

impl<A> TryFrom<&SerializedEvent> for EventEnvelope<A>
where
    A: Aggregate,
{
    type Error = serde_json::Error;

    fn try_from(value: &SerializedEvent) -> Result<Self, Self::Error> {
        let metadata = Metadata::builder()
            .aggregate_id(value.aggregate_id.clone())
            .aggregate_type(value.aggregate_type.clone())
            .occurred_at(value.occurred_at)
            .build();

        let payload: A::Event = serde_json::from_value(value.payload.clone())?;

        let context = BusinessContext::builder()
            .maybe_correlation_id(value.correlation_id.clone())
            .maybe_causation_id(value.causation_id.clone())
            .maybe_actor_type(value.actor_type.clone())
            .maybe_actor_id(value.actor_id.clone())
            .build();

        Ok(EventEnvelope {
            metadata,
            payload,
            context,
        })
    }
}

pub fn serialize_events<A>(events: &[EventEnvelope<A>]) -> Result<Vec<SerializedEvent>>
where
    A: Aggregate,
{
    let events = events
        .iter()
        .map(|e| SerializedEvent::try_from(e))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(events)
}

pub fn deserialize_events<A>(
    upcaster_chain: &EventUpcasterChain,
    events: Vec<SerializedEvent>,
) -> Result<Vec<EventEnvelope<A>>>
where
    A: Aggregate,
{
    let events = upcaster_chain.upcast_all(events)?;

    let events = events
        .iter()
        .map(|e| EventEnvelope::try_from(e))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(events)
}
