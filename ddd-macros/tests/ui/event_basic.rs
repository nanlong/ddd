use ddd_macros::event;
use serde::{Deserialize, Serialize};

#[event(version = 1)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum BankEvent {
    #[event_type = "bank.opened"]
    Opened { id: String, aggregate_version: usize, name: String },
    #[event_type = "bank.renamed"]
    Renamed { id: String, aggregate_version: usize, to: String },
}

fn main() {}

