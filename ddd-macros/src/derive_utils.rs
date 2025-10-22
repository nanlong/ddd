use quote::ToTokens;
use syn::{Attribute, Token};

// 提取非 derive 属性与已有 derive 列表
pub(crate) fn split_derives(attrs: &[Attribute]) -> (Vec<Attribute>, Vec<syn::Path>) {
    let mut retained = Vec::new();
    let mut existing = Vec::new();
    for attr in attrs.iter() {
        if attr.path().is_ident("derive") {
            if let Ok(list) = attr.parse_args_with(
                syn::punctuated::Punctuated::<syn::Path, Token![,]>::parse_terminated,
            ) {
                for p in list.into_iter() {
                    existing.push(p);
                }
            }
        } else {
            retained.push(attr.clone());
        }
    }
    (retained, existing)
}

// 合并默认与已有 derive（去重，优先保留 required）
pub(crate) fn merge_derives(existing: Vec<syn::Path>, required: Vec<syn::Path>) -> Attribute {
    let mut seen = std::collections::HashSet::<String>::new();
    let mut final_list: Vec<syn::Path> = Vec::new();
    let mut push_unique = |p: syn::Path| {
        let key = derive_key(&p);
        if seen.insert(key) {
            final_list.push(p);
        }
    };
    for p in required {
        push_unique(p);
    }
    for p in existing {
        push_unique(p);
    }
    syn::parse_quote!(#[derive(#(#final_list),*)])
}

// 归一化 derive 的 key，避免 Serialize/serde::Serialize 重复
pub(crate) fn derive_key(p: &syn::Path) -> String {
    if let Some(last) = p.segments.last() {
        let last_ident = last.ident.to_string();
        match last_ident.as_str() {
            "Serialize" | "Deserialize" => format!("serde::{}", last_ident),
            _ => last_ident,
        }
    } else {
        p.to_token_stream().to_string()
    }
}

// 直接在 attrs 上应用默认派生合并
pub(crate) fn apply_derives(attrs: &mut Vec<Attribute>, required: Vec<syn::Path>) {
    let (retained, existing) = split_derives(attrs);
    let merged = merge_derives(existing, required);
    *attrs = std::iter::once(merged).chain(retained).collect();
}
