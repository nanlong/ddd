use proc_macro::TokenStream;
use quote::{ToTokens, quote};
use std::collections::HashMap;
use syn::Token;
use syn::punctuated::Punctuated;
use syn::{
    Expr, Ident, Item, ItemStruct, Result as SynResult, Type, parse::Parse, parse::ParseStream,
    parse_macro_input, spanned::Spanned,
};

/// 实体宏（原 aggregate 宏）
/// - 追加字段：`id: IdType`, `version: usize`（若缺失）并置于字段最前
/// - 自动为目标结构体实现 `::ddd_domain::entiry::Entity` trait（`new/id/version`）
/// - 支持参数：`#[entity(id = IdType)]`，默认 `String`
#[proc_macro_attribute]
pub fn entity(attr: TokenStream, item: TokenStream) -> TokenStream {
    let cfg = parse_macro_input!(attr as EntityAttrConfig);
    let input = parse_macro_input!(item as Item);

    let mut st = match input {
        Item::Struct(s) => s,
        other => {
            return syn::Error::new(other.span(), "#[entity] only on struct")
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

    let out_struct = ItemStruct { ..st };

    // 为结构体生成 Entity 实现
    let ident = &out_struct.ident;
    let generics = out_struct.generics.clone();
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let expanded = quote! {
        #out_struct

        impl #impl_generics ::ddd_domain::entiry::Entity for #ident #ty_generics #where_clause {
            type Id = #id_type;

            fn new(aggregate_id: Self::Id) -> Self {
                Self {
                    id: aggregate_id,
                    version: 0,
                    ..Default::default()
                }
            }

            fn id(&self) -> &Self::Id {
                &self.id
            }

            fn version(&self) -> usize {
                self.version
            }
        }
    };

    TokenStream::from(expanded)
}

/// 实体 ID 宏
/// 用于为 `tuple struct` 形式的 ID 类型（例如 `struct AccountId(String);`、`struct OrderId(Uuid);`）
/// 自动实现以下 trait：
/// - `Display`（要求内部类型实现 `Display`）
/// - `FromStr`（要求内部类型实现 `FromStr`，并委托解析）
/// 仅支持单字段的 `tuple struct`。
#[proc_macro_attribute]
pub fn entity_id(attr: TokenStream, item: TokenStream) -> TokenStream {
    let _ = attr; // 暂不支持属性参数
    let input = parse_macro_input!(item as Item);

    let st = match input {
        Item::Struct(s) => s,
        other => {
            return syn::Error::new(other.span(), "#[entity_id] only on struct")
                .to_compile_error()
                .into();
        }
    };

    let fields = match &st.fields {
        syn::Fields::Unnamed(f) if f.unnamed.len() == 1 => f,
        syn::Fields::Unnamed(f) => {
            return syn::Error::new(
                f.span(),
                "#[entity_id] requires a tuple struct with exactly one field",
            )
            .to_compile_error()
            .into();
        }
        _ => {
            return syn::Error::new(
                st.span(),
                "#[entity_id] supports only tuple struct, e.g., struct X(String);",
            )
            .to_compile_error()
            .into();
        }
    };

    let inner_ty = &fields.unnamed.first().unwrap().ty;

    let ident = &st.ident;
    let generics = st.generics.clone();
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let out = quote! {
        #st

        impl #impl_generics ::std::str::FromStr for #ident #ty_generics #where_clause
        where #inner_ty: ::std::str::FromStr
        {
            type Err = <#inner_ty as ::std::str::FromStr>::Err;
            fn from_str(s: &str) -> ::std::result::Result<Self, Self::Err> {
                let inner: #inner_ty = s.parse()?;
                ::std::result::Result::Ok(Self(inner))
            }
        }

        impl #impl_generics ::std::fmt::Display for #ident #ty_generics #where_clause
        where #inner_ty: ::std::fmt::Display
        {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                ::std::write!(f, "{}", self.0)
            }
        }
    };

    TokenStream::from(out)
}

/// 仅支持形如：
/// pub enum XxxEvent {
///     Variant { field_a: T, ... },
/// }
/// 的具名字段变体，并为每个变体追加 id: IdType, version: usize 两个字段（若缺失）。
/// 支持键值形式：
/// - #[event(id = IdType)] 指定 id 字段类型（默认 String）
/// - #[event(version = N)] 指定 DomainEvent::event_version 返回的默认版本号（默认 1）
/// - 变体可通过 #[event(type = "...", version = N)] 覆写事件类型与版本号
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

    // 变体 -> 自定义事件类型名 / 版本
    let mut variant_types: HashMap<String, syn::LitStr> = HashMap::new();
    let mut variant_versions: HashMap<String, syn::LitInt> = HashMap::new();

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

                let mut retained_attrs = Vec::new();
                let mut type_lit: Option<syn::LitStr> = None;
                let mut version_lit: Option<syn::LitInt> = None;

                for attr in v.attrs.iter() {
                    if attr.path().is_ident("event") {
                        match parse_variant_event_attr(attr) {
                            Ok(cfg) => {
                                if let Some(lit) = cfg.ty {
                                    if type_lit.is_some() {
                                        return syn::Error::new(
                                            attr.span(),
                                            "duplicate 'event_type' specified for this variant",
                                        )
                                        .to_compile_error()
                                        .into();
                                    }
                                    type_lit = Some(lit);
                                }
                                if let Some(lit) = cfg.version {
                                    if version_lit.is_some() {
                                        return syn::Error::new(
                                            attr.span(),
                                            "duplicate 'event_version' specified for this variant",
                                        )
                                        .to_compile_error()
                                        .into();
                                    }
                                    version_lit = Some(lit);
                                }
                            }
                            Err(err) => {
                                return err.to_compile_error().into();
                            }
                        }
                    } else if attr.path().is_ident("event_type")
                        || attr.path().is_ident("event_version")
                    {
                        return syn::Error::new(
                            attr.span(),
                            "legacy #[event_type]/#[event_version] syntax is no longer supported; use #[event(event_type = ..., event_version = ...)]",
                        )
                        .to_compile_error()
                        .into();
                    } else {
                        retained_attrs.push(attr.clone());
                    }
                }

                v.attrs = retained_attrs;

                if let Some(lit) = type_lit {
                    variant_types.insert(v.ident.to_string(), lit);
                }

                if let Some(lit) = version_lit {
                    variant_versions.insert(v.ident.to_string(), lit);
                }
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
            quote! { Self::#v_ident { .. } => #lit }
        } else {
            let combined = format!("{}.{}", enum_name_string, key);
            let lit = syn::LitStr::new(&combined, v_ident.span());
            quote! { Self::#v_ident { .. } => #lit }
        }
    });

    let id_match_arms = enum_item.variants.iter().map(|v| {
        let v_ident = &v.ident;
        quote! { Self::#v_ident { id, .. } => id.as_str() }
    });

    let ver_match_arms = enum_item.variants.iter().map(|v| {
        let v_ident = &v.ident;
        let key = v_ident.to_string();
        if let Some(lit) = variant_versions.get(&key) {
            quote! { Self::#v_ident { .. } => #lit }
        } else {
            quote! { Self::#v_ident { .. } => #version_lit }
        }
    });

    let agg_ver_match_arms = enum_item.variants.iter().map(|v| {
        let v_ident = &v.ident;
        quote! { Self::#v_ident { aggregate_version, .. } => *aggregate_version }
    });

    let out = quote! {
        #enum_item

        impl ::ddd_domain::domain_event::DomainEvent for #enum_ident {
            fn event_id(&self) -> &str {
                match self { #( #id_match_arms, )* }
            }

            fn event_type(&self) -> &str {
                match self { #( #type_match_arms, )* }
            }

            fn event_version(&self) -> usize {
                match self { #( #ver_match_arms, )* }
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

struct VariantEventAttrConfig {
    ty: Option<syn::LitStr>,
    version: Option<syn::LitInt>,
}

fn parse_variant_event_attr(attr: &syn::Attribute) -> SynResult<VariantEventAttrConfig> {
    match &attr.meta {
        syn::Meta::List(_) => {
            let mut ty: Option<syn::LitStr> = None;
            let mut version: Option<syn::LitInt> = None;
            let pairs: Punctuated<VariantEventAttrKv, Token![,]> = attr
                .parse_args_with(Punctuated::<VariantEventAttrKv, Token![,]>::parse_terminated)?;

            for kv in pairs {
                let key = kv.key.to_string();
                match key.as_str() {
                    "event_type" => {
                        if ty.is_some() {
                            return Err(syn::Error::new(
                                kv.key.span(),
                                "duplicate key 'event_type' in attribute",
                            ));
                        }
                        let lit = match kv.value {
                            Expr::Lit(syn::ExprLit {
                                lit: syn::Lit::Str(lit),
                                ..
                            }) => lit,
                            other => {
                                return Err(syn::Error::new(
                                    other.span(),
                                    "expected string literal for 'event_type'",
                                ));
                            }
                        };
                        ty = Some(lit);
                    }
                    "event_version" => {
                        if version.is_some() {
                            return Err(syn::Error::new(
                                kv.key.span(),
                                "duplicate key 'event_version' in attribute",
                            ));
                        }
                        let lit = match kv.value {
                            Expr::Lit(syn::ExprLit {
                                lit: syn::Lit::Int(lit),
                                ..
                            }) => lit,
                            other => {
                                return Err(syn::Error::new(
                                    other.span(),
                                    "expected integer literal for 'event_version'",
                                ));
                            }
                        };
                        version = Some(lit);
                    }
                    _ => {
                        return Err(syn::Error::new(
                            kv.key.span(),
                            "unknown key; expected 'event_type' | 'event_version'",
                        ));
                    }
                }
            }

            Ok(VariantEventAttrConfig { ty, version })
        }
        other => Err(syn::Error::new(other.span(), "expected #[event(...)]")),
    }
}

struct VariantEventAttrKv {
    key: Ident,
    #[allow(dead_code)]
    eq: Token![=],
    value: Expr,
}

impl Parse for VariantEventAttrKv {
    fn parse(input: ParseStream) -> SynResult<Self> {
        Ok(Self {
            key: input.parse()?,
            eq: input.parse()?,
            value: input.parse()?,
        })
    }
}

// 解析 entity 宏键值参数：id = <Type>
struct EntityAttrConfig {
    id_ty: Option<Type>,
}

impl Parse for EntityAttrConfig {
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
