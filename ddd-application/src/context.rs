use ddd_domain::domain_event::BusinessContext;

#[derive(Clone, Debug, Default)]
pub struct AppContext {
    pub biz: BusinessContext,
    pub idempotency_key: Option<String>,
}
