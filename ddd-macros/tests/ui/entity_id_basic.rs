use ddd_macros::entity_id;

#[entity_id]
struct UserId(String);

#[entity_id(debug = false)]
struct ProfileId(String);

impl std::fmt::Debug for ProfileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ProfileId(..)")
    }
}

fn main() {
    let id = UserId::new("u1".to_string());
    let _ = format!("{:?}", id); // 默认启用 Debug，应可用

    let pid = ProfileId::new("p1".into());
    let _ = format!("{:?}", pid); // 使用手写 Debug，实现可编译则说明未自动派生 Debug
}
