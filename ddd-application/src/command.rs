pub trait Command: Send + Sync + 'static {
    const NAME: &'static str;
}
