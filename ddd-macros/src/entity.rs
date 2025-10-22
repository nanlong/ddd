use crate::derive_utils::{merge_derives, split_derives};
use proc_macro::TokenStream;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{
    Item, ItemStruct, Result, Token, Type, parse::Parse, parse::ParseStream, parse_macro_input,
};

/// #[entity] 宏实现
/// - 若缺失则追加字段：`id: IdType`, `version: usize`，并置于字段最前
/// - 自动实现 `::ddd_domain::entity::Entity`（new/id/version）
/// - 支持参数：`#[entity(id = IdType)]`，默认 `String`
pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
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

    // 仅支持具名字段结构体
    let fields_named = match &mut st.fields {
        syn::Fields::Named(f) => f,
        _ => {
            return syn::Error::new(st.span(), "only supports named-field struct")
                .to_compile_error()
                .into();
        }
    };

    let id_type = cfg.id_ty.unwrap_or_else(|| syn::parse_quote! { String });

    // 重新组织字段：确保 id/version 在最前，并避免重复
    let mut new_named: Punctuated<syn::Field, Token![,]> = Punctuated::new();

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

    if let Some(f) = existed_id {
        new_named.push(f);
    } else {
        new_named.push(syn::parse_quote! { id: #id_type });
    }

    if let Some(f) = existed_version {
        new_named.push(f);
    } else {
        new_named.push(syn::parse_quote! { version: usize });
    }

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

    // 合并/规范 derive：默认添加 Debug, Default, Serialize, Deserialize，且允许用户在原有基础上追加
    let (retained, existing_derives) = split_derives(&st.attrs);
    let required: Vec<syn::Path> = vec![
        syn::parse_quote!(Debug),
        syn::parse_quote!(Default),
        syn::parse_quote!(serde::Serialize),
        syn::parse_quote!(serde::Deserialize),
    ];
    let merged = merge_derives(existing_derives, required);
    st.attrs = std::iter::once(merged).chain(retained).collect();

    let out_struct = ItemStruct { ..st };

    // 生成 Entity 实现
    let ident = &out_struct.ident;
    let generics = out_struct.generics.clone();
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let expanded = quote! {
        #out_struct

        impl #impl_generics ::ddd_domain::entity::Entity for #ident #ty_generics #where_clause {
            type Id = #id_type;

            fn new(aggregate_id: Self::Id, version: usize) -> Self {
                Self { id: aggregate_id, version, ..Default::default() }
            }

            fn id(&self) -> &Self::Id { &self.id }

            fn version(&self) -> usize { self.version }
        }
    };

    TokenStream::from(expanded)
}

// -------- parsing --------

struct EntityAttrConfig {
    id_ty: Option<Type>,
}

impl Parse for EntityAttrConfig {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut id_ty: Option<Type> = None;

        if input.is_empty() {
            return Ok(Self { id_ty });
        }

        let pairs: Punctuated<KvType, Token![,]> =
            Punctuated::<KvType, Token![,]>::parse_terminated(input)?;

        for kv in pairs.into_iter() {
            match kv.key.to_string().as_str() {
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
    key: syn::Ident,
    #[allow(dead_code)]
    eq: Token![=],
    ty: Type,
}

impl Parse for KvType {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {
            key: input.parse()?,
            eq: input.parse()?,
            ty: input.parse()?,
        })
    }
}
