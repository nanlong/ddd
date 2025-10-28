use ddd_macros::domain_event;
use serde::{Deserialize, Serialize};

#[domain_event(version = 1)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum BankEvent {
    #[event(event_type = "bank.opened")]
    Opened {
        id: String,
        aggregate_version: usize,
        name: String,
    },
    #[event(event_type = "bank.renamed", event_version = 2)]
    Renamed {
        id: String,
        aggregate_version: usize,
        to: String,
    },
}

fn main() {}
