use std::{fmt::Display, str::FromStr};

pub trait Entity: Send + Sync {
    type Id: FromStr + Clone + Display;

    fn new(aggregate_id: Self::Id) -> Self;

    fn id(&self) -> &Self::Id;

    fn version(&self) -> usize;
}
