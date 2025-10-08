use crate::dto::Dto;

pub trait Query: Send + Sync + 'static {
    const NAME: &'static str;

    type Dto: Dto;
}
