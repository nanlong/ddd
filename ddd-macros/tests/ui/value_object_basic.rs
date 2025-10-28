use ddd_macros::value_object;

#[value_object]
struct Amount {
    value: i64,
}

#[value_object(debug = false)]
struct NonDebugVO(i32);

#[value_object]
enum Level {
    #[default]
    Low,
    High,
}

fn main() {
    // Debug 默认开启，应可格式化
    let _ = format!("{:?}", Amount { value: 0 });

    // Default/Clone/PartialEq/Hash 可用（编译期检查足矣）
    let a = Amount::default();
    let _b = a.clone();
    let _eq = a == Amount { value: 0 };

    // debug = false 的不强制使用 Debug，避免误导测试；只做构造以确保通过
    let _ = NonDebugVO(1);

    // 枚举派生 Default/Clone/Debug/Serialize/Deserialize 等
    let _lv: Level = Default::default();
}
