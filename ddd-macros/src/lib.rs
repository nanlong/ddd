//! DDD 辅助宏（拆分模块版）
//! - 每个宏放置在独立文件，根仅做入口与转发

use proc_macro::TokenStream;

mod entity;
mod entity_id;
mod event;

/// 实体宏（原 aggregate）
#[proc_macro_attribute]
pub fn entity(attr: TokenStream, item: TokenStream) -> TokenStream {
    entity::expand(attr, item)
}

/// 实体 ID 宏（tuple struct 新类型）
#[proc_macro_attribute]
pub fn entity_id(attr: TokenStream, item: TokenStream) -> TokenStream {
    entity_id::expand(attr, item)
}

/// 领域事件宏
#[proc_macro_attribute]
pub fn event(attr: TokenStream, item: TokenStream) -> TokenStream {
    event::expand(attr, item)
}
