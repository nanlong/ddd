use proc_macro::TokenStream;
use quote::{ToTokens, quote};
use std::collections::HashMap;
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
/// 支持键值形式：
/// - #[event(id = IdType)] 指定 id 字段类型（默认 String）
/// - #[event(version = N)] 指定 DomainEvent::event_version 返回的常量版本（默认 1）
/// - 事件类型应在“变体”上通过 #[event_type = "..."] 指定；若缺省，则默认使用 "枚举名.变体名"
/// - 不再兼容变体级 #[event(type = "...")] 写法，发现则直接报错提示改用 #[event_type]
#[proc_macro_attribute]
pub fn event(attr: TokenStream, item: TokenStream) -> TokenStream {
    let cfg = parse_macro_input!(attr as EventAttrConfig);
    let mut input = parse_macro_input!(item as Item);

    let enum_item = match &mut input {
        Item::Enum(e) => e,
        other => {
            return syn::Error::new(other.span(), "#[event] can only be used on enum types")
                .to_compile_error()
                .into();
        }
    };

    // 配置：id 类型 / 事件版本（禁止在枚举上设置全局 type）
    let id_type = cfg.id_ty.unwrap_or_else(|| syn::parse_quote! { String });
    let version_lit = cfg.version.unwrap_or_else(|| syn::parse_quote! { 1 });

    // 变体 -> 自定义事件类型名
    let mut variant_types: HashMap<String, syn::LitStr> = HashMap::new();

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

                if let Some(lit) = read_variant_type_attr(&v.attrs) {
                    variant_types.insert(v.ident.to_string(), lit);
                }

                // 清理变体上的辅助属性（仅移除本宏识别的 event_type / 旧 event）
                v.attrs
                    .retain(|a| !(a.path().is_ident("event_type") || a.path().is_ident("event")));
            }
            _ => {
                return syn::Error::new(
                    v.span(),
                    "#[event] supports only named-field enum variants, e.g., Variant { x: T }",
                )
                .to_compile_error()
                .into();
            }
        }
    }

    // 生成 DomainEvent 实现
    let enum_ident = &enum_item.ident;
    let enum_name_string = enum_ident.to_string();
    let type_match_arms = enum_item.variants.iter().map(|v| {
        let v_ident = &v.ident;
        let key = v_ident.to_string();
        // 变体级覆盖或默认：EnumName.Variant
        if let Some(lit) = variant_types.get(&key) {
            quote! { Self::#v_ident { .. } => #lit.to_string() }
        } else {
            let combined = format!("{}.{}", enum_name_string, key);
            let lit = syn::LitStr::new(&combined, v_ident.span());
            quote! { Self::#v_ident { .. } => #lit.to_string() }
        }
    });

    let id_match_arms = enum_item.variants.iter().map(|v| {
        let v_ident = &v.ident;
        quote! { Self::#v_ident { id, .. } => id.clone() }
    });

    let ver_match_arm = quote! { _ => #version_lit };

    let agg_ver_match_arms = enum_item.variants.iter().map(|v| {
        let v_ident = &v.ident;
        quote! { Self::#v_ident { aggregate_version, .. } => *aggregate_version }
    });

    let out = quote! {
        #enum_item

        impl ::ddd::domain_event::DomainEvent for #enum_ident {
            fn event_id(&self) -> String {
                match self { #( #id_match_arms, )* }
            }
            fn event_type(&self) -> String {
                match self { #( #type_match_arms, )* }
            }
            fn event_version(&self) -> usize {
                match self { #ver_match_arm }
            }
            fn aggregate_version(&self) -> usize {
                match self { #( #agg_ver_match_arms, )* }
            }
        }
    };

    TokenStream::from(out)
}

// 判断具名字段结构体中是否存在指定字段名
fn has_field_named(fields: &syn::FieldsNamed, name: &str) -> bool {
    fields
        .named
        .iter()
        .any(|f| f.ident.as_ref().map(|i| i == name).unwrap_or(false))
}

// 提取变体级事件类型：仅支持 #[event_type = "..."]
fn read_variant_type_attr(attrs: &[syn::Attribute]) -> Option<syn::LitStr> {
    for attr in attrs {
        if attr.path().is_ident("event_type")
            && let syn::Meta::NameValue(nv) = &attr.meta
                && let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(lit),
                    ..
                }) = &nv.value
                {
                    return Some(lit.clone());
                }
    }
    None
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

// 解析 event 宏键值参数：id = <Type>、version = <int>
struct EventAttrConfig {
    id_ty: Option<Type>,
    version: Option<syn::LitInt>,
}

impl Parse for EventAttrConfig {
    fn parse(input: ParseStream) -> SynResult<Self> {
        let mut id_ty: Option<Type> = None;
        let mut version: Option<syn::LitInt> = None;

        if input.is_empty() {
            return Ok(Self { id_ty, version });
        }

        let pairs: Punctuated<syn::ExprAssign, Token![,]> =
            Punctuated::<syn::ExprAssign, Token![,]>::parse_terminated(input)?;

        for assign in pairs.into_iter() {
            let key_ident = match *assign.left {
                syn::Expr::Path(p) if p.path.segments.len() == 1 => {
                    p.path.segments[0].ident.clone()
                }
                other => {
                    return Err(syn::Error::new(other.span(), "invalid attribute key"));
                }
            };
            match key_ident.to_string().as_str() {
                "id" => {
                    if id_ty.is_some() {
                        return Err(syn::Error::new(
                            key_ident.span(),
                            "duplicate key 'id' in attribute",
                        ));
                    }
                    let ty_parsed: Type = syn::parse2(assign.right.to_token_stream())?;
                    id_ty = Some(ty_parsed);
                }
                "version" => {
                    if version.is_some() {
                        return Err(syn::Error::new(
                            key_ident.span(),
                            "duplicate key 'version' in attribute",
                        ));
                    }
                    let lit: syn::LitInt = syn::parse2(assign.right.to_token_stream())?;
                    version = Some(lit);
                }
                _ => {
                    return Err(syn::Error::new(
                        key_ident.span(),
                        "unknown key; expected 'id' | 'version'",
                    ));
                }
            }
        }

        Ok(Self { id_ty, version })
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
