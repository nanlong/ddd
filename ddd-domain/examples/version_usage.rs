//! Version 新类型使用示例
//!
//! 展示如何使用 `Version` 类型来增强类型安全性和代码可读性。
//!
//! 运行示例：
//! ```bash
//! cargo run -p ddd-domain --example version_usage
//! ```

use ddd_domain::value_object::Version;

fn main() {
    println!("=== Version 新类型使用示例 ===\n");

    // 1. 创建初始版本
    println!("1. 创建初始版本");
    let v0 = Version::new();
    println!("   初始版本: {} (value = {})", v0, v0.value());
    println!("   是否为初始版本: {}", v0.is_new());
    println!("   是否已创建: {}\n", v0.is_created());

    // 2. 创建指定版本号
    println!("2. 从值创建版本");
    let v5 = Version::from_value(5);
    println!("   版本: {} (value = {})", v5, v5.value());
    println!("   是否为初始版本: {}", v5.is_new());
    println!("   是否已创建: {}\n", v5.is_created());

    // 3. 获取下一个版本
    println!("3. 版本递增");
    let v1 = v0.next();
    let v2 = v1.next();
    println!("   v0.next() = {}", v1);
    println!("   v1.next() = {}", v2);
    println!(
        "   链式调用: v0.next().next().next() = {}\n",
        v0.next().next().next()
    );

    // 4. 不可变递增（值对象特性）
    println!("4. 不可变递增（值对象特性）");
    let version = Version::new();
    println!("   初始: {}", version);
    let version = version.next();
    println!("   递增后: {}", version);
    let version = version.next();
    println!("   再次递增: {}\n", version);

    // 5. 版本比较
    println!("5. 版本比较");
    let v10 = Version::from_value(10);
    let v20 = Version::from_value(20);
    println!("   v10 = {}, v20 = {}", v10, v20);
    println!("   v10 < v20: {}", v10 < v20);
    println!("   v20 > v10: {}", v20 > v10);
    println!(
        "   v10 == Version::from_value(10): {}\n",
        v10 == Version::from_value(10)
    );

    // 6. 类型转换
    println!("6. 类型转换");
    let version_from_usize: Version = 42.into();
    println!("   从 usize 创建: 42 -> {}", version_from_usize);

    let usize_from_version: usize = version_from_usize.into();
    println!(
        "   转换回 usize: {} -> {}\n",
        version_from_usize, usize_from_version
    );

    // 7. 序列化和反序列化
    println!("7. 序列化和反序列化");
    let v100 = Version::from_value(100);
    let json = serde_json::to_string(&v100).unwrap();
    println!("   序列化: {} -> {}", v100, json);

    let deserialized: Version = serde_json::from_str(&json).unwrap();
    println!("   反序列化: {} -> {}\n", json, deserialized);

    // 8. 实际应用场景：聚合版本管理
    println!("8. 实际应用：聚合版本管理");
    simulate_aggregate_version_management();

    println!("\n=== 示例完成 ===");
}

/// 模拟聚合版本管理场景
fn simulate_aggregate_version_management() {
    #[derive(Debug)]
    struct Aggregate {
        id: String,
        version: Version,
        data: String,
    }

    impl Aggregate {
        fn new(id: String) -> Self {
            Self {
                id,
                version: Version::new(),
                data: String::new(),
            }
        }

        fn apply_event(&mut self, event: &str) {
            self.data.push_str(event);
            let old_version = self.version;
            self.version = self.version.next();
            println!(
                "   [{}] 应用事件: '{}', 版本: {} -> {}",
                self.id, event, old_version, self.version
            );
        }

        fn is_new(&self) -> bool {
            self.version.is_new()
        }

        fn current_version(&self) -> Version {
            self.version
        }
    }

    // 创建新聚合
    let mut aggregate = Aggregate::new("account-001".to_string());
    println!("   创建聚合: {:?}", aggregate.id);
    println!("   是否为新聚合: {}", aggregate.is_new());
    println!("   当前版本: {}", aggregate.current_version());

    // 应用一系列事件
    println!("\n   应用事件序列:");
    aggregate.apply_event("账户已开通");
    aggregate.apply_event("存入 1000 元");
    aggregate.apply_event("取出 500 元");

    println!("\n   最终状态:");
    println!("   聚合 ID: {}", aggregate.id);
    println!("   当前版本: {}", aggregate.current_version());
    println!("   是否为新聚合: {}", aggregate.is_new());
    println!("   事件数量: {}", aggregate.version.value());

    // 版本冲突检测
    println!("\n   版本冲突检测:");
    let expected_version = Version::from_value(2);
    let actual_version = aggregate.current_version();

    if actual_version != expected_version {
        println!(
            "   ⚠️  版本冲突: 期望 {}, 实际 {}",
            expected_version, actual_version
        );
    } else {
        println!("   ✅ 版本一致");
    }
}
