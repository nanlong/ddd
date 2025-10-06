use ddd_macros::aggregate;
use serde::{Deserialize, Serialize};

#[aggregate(id = String)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Account {
    name: String,
}

fn main() {}
