use proc_macro::TokenStream;
use quote::quote;
use syn::Token;
use syn::punctuated::Punctuated;
use syn::{
    Ident, Item, ItemStruct, Result as SynResult, Type, parse::Parse, parse::ParseStream,
    parse_macro_input, spanned::Spanned,
};

/// 聚合根宏
/// 追加 id: IdType, version: usize 两个字段（若缺失）
/// 支持键值形式：#[aggregate(id = IdType)]，若不指定则默认为 String。
#[proc_macro_attribute]
pub fn aggregate(attr: TokenStream, item: TokenStream) -> TokenStream {
    let cfg = parse_macro_input!(attr as AggregateAttrConfig);
    let input = parse_macro_input!(item as Item);

    let mut st = match input {
        Item::Struct(s) => s,
        other => {
            return syn::Error::new(other.span(), "#[aggregate] only on struct")
                .to_compile_error()
                .into();
        }
    };

    // 仅支持具名字段
    let fields_named = match &mut st.fields {
        syn::Fields::Named(f) => f,
        _ => {
            return syn::Error::new(st.span(), "only supports named-field struct")
                .to_compile_error()
                .into();
        }
    };

    // 确定使用的 id 类型
    let id_type = cfg.id_ty.unwrap_or_else(|| syn::parse_quote! { String });

    // 重建字段顺序：将 id、version 放在最前，其他字段保持原有相对顺序
    let mut new_named: Punctuated<syn::Field, Token![,]> = Punctuated::new();

    // 取出现有 id/version 字段（若存在则复用原定义）
    let existed_id = fields_named
        .named
        .iter()
        .find(|f| f.ident.as_ref().map(|i| i == "id").unwrap_or(false))
        .cloned();

    let existed_version = fields_named
        .named
        .iter()
        .find(|f| f.ident.as_ref().map(|i| i == "version").unwrap_or(false))
        .cloned();

    // id：若存在则放前面；若不存在则使用配置的类型（默认 String）新增
    if let Some(f) = existed_id {
        new_named.push(f);
    } else {
        new_named.push(syn::parse_quote! { id: #id_type });
    }

    // version：若存在则放前面；若不存在则新增并放前面
    if let Some(f) = existed_version {
        new_named.push(f);
    } else {
        new_named.push(syn::parse_quote! { version: usize });
    }

    // 其他字段按原来顺序追加，但跳过 id/version，避免重复
    for f in fields_named.named.clone().into_iter() {
        let is_id_or_version = f
            .ident
            .as_ref()
            .map(|i| i == "id" || i == "version")
            .unwrap_or(false);
        if !is_id_or_version {
            new_named.push(f);
        }
    }

    fields_named.named = new_named;

    let out = ItemStruct { ..st };
    TokenStream::from(quote! { #out })
}

/// 仅支持形如：
/// pub enum XxxEvent {
///     Variant { field_a: T, ... },
/// }
/// 的具名字段变体，并为每个变体追加 id: IdType, version: usize 两个字段（若缺失）。
/// 支持键值形式：#[event(id = IdType)]，若不指定则默认为 String。
#[proc_macro_attribute]
pub fn event(attr: TokenStream, item: TokenStream) -> TokenStream {
    let cfg = parse_macro_input!(attr as EventAttrConfig);
    let mut input = parse_macro_input!(item as Item);

    let enum_item = match &mut input {
        Item::Enum(e) => e,
        other => {
            return syn::Error::new(other.span(), "#[event] 只能用于 enum 类型")
                .to_compile_error()
                .into();
        }
    };

    // 确定使用的 id 类型
    let id_type = cfg.id_ty.unwrap_or_else(|| syn::parse_quote! { String });

    for v in &mut enum_item.variants {
        match &mut v.fields {
            syn::Fields::Named(fields_named) => {
                let mut new_named: Punctuated<syn::Field, Token![,]> = Punctuated::new();

                // 如果缺失则添加 id、version 字段
                if !has_field_named(fields_named, "id") {
                    new_named.push(syn::parse_quote! { id: #id_type });
                }

                if !has_field_named(fields_named, "aggregate_version") {
                    new_named.push(syn::parse_quote! { aggregate_version: usize });
                }

                // 保留原有字段顺序
                for f in fields_named.named.clone().into_iter() {
                    new_named.push(f);
                }

                fields_named.named = new_named;
            }
            _ => {
                return syn::Error::new(
                    v.span(),
                    "#[event] 仅支持具名字段的枚举变体，如 Variant { x: T }",
                )
                .to_compile_error()
                .into();
            }
        }
    }

    TokenStream::from(quote! { #enum_item })
}

// 判断具名字段结构体中是否存在指定字段名
fn has_field_named(fields: &syn::FieldsNamed, name: &str) -> bool {
    fields
        .named
        .iter()
        .any(|f| f.ident.as_ref().map(|i| i == name).unwrap_or(false))
}

// 解析 aggregate 宏键值参数：id = <Type>
struct AggregateAttrConfig {
    id_ty: Option<Type>,
}

impl Parse for AggregateAttrConfig {
    fn parse(input: ParseStream) -> SynResult<Self> {
        let mut id_ty: Option<Type> = None;

        if input.is_empty() {
            return Ok(Self { id_ty });
        }

        let pairs: Punctuated<KvType, Token![,]> =
            Punctuated::<KvType, Token![,]>::parse_terminated(input)?;

        for kv in pairs.into_iter() {
            let key = kv.key.to_string();
            match key.as_str() {
                "id" => {
                    if id_ty.is_some() {
                        return Err(syn::Error::new(
                            kv.key.span(),
                            "duplicate key 'id' in attribute",
                        ));
                    }
                    id_ty = Some(kv.ty);
                }
                _ => {
                    return Err(syn::Error::new(
                        kv.key.span(),
                        "unknown key in attribute; expected 'id'",
                    ));
                }
            }
        }

        Ok(Self { id_ty })
    }
}

// 解析 event 宏键值参数：id = <Type>
struct EventAttrConfig {
    id_ty: Option<Type>,
}

impl Parse for EventAttrConfig {
    fn parse(input: ParseStream) -> SynResult<Self> {
        let mut id_ty: Option<Type> = None;

        if input.is_empty() {
            return Ok(Self { id_ty });
        }

        let pairs: Punctuated<KvType, Token![,]> =
            Punctuated::<KvType, Token![,]>::parse_terminated(input)?;

        for kv in pairs.into_iter() {
            let key = kv.key.to_string();
            match key.as_str() {
                "id" => {
                    if id_ty.is_some() {
                        return Err(syn::Error::new(
                            kv.key.span(),
                            "duplicate key 'id' in attribute",
                        ));
                    }
                    id_ty = Some(kv.ty);
                }
                _ => {
                    return Err(syn::Error::new(
                        kv.key.span(),
                        "unknown key in attribute; expected 'id'",
                    ));
                }
            }
        }

        Ok(Self { id_ty })
    }
}

struct KvType {
    key: Ident,
    #[allow(dead_code)]
    eq: Token![=],
    ty: Type,
}

impl Parse for KvType {
    fn parse(input: ParseStream) -> SynResult<Self> {
        let key: Ident = input.parse()?;
        let eq: Token![=] = input.parse()?;
        let ty: Type = input.parse()?;
        Ok(Self { key, eq, ty })
    }
}
