use ddd_macros::event;
use serde::{Deserialize, Serialize};

#[event(version = 1)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum UserEvent {
    Created { id: String, aggregate_version: usize },
}

fn main() {}

