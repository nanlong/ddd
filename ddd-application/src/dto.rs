use serde::Serialize;

pub trait Dto: Serialize + Send + Sync + 'static {}
