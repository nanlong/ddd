use crate::derive_utils::apply_derives;
use proc_macro::TokenStream;
use quote::quote;
use syn::spanned::Spanned;
use syn::{Item, parse_macro_input};

/// #[entity_id] 宏实现
/// 仅支持单字段 tuple struct，并为包装类型：
/// - 合并/追加派生：Default, Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash
/// - 提供 new(value)、Display、FromStr、AsRef/AsMut、From 等便捷实现
pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    let _ = attr; // 暂无参数
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

    // 合并/规范 derive
    let mut st_out = st.clone();
    let required: Vec<syn::Path> = vec![
        syn::parse_quote!(Default),
        syn::parse_quote!(Clone),
        syn::parse_quote!(Debug),
        syn::parse_quote!(serde::Serialize),
        syn::parse_quote!(serde::Deserialize),
        syn::parse_quote!(PartialEq),
        syn::parse_quote!(Eq),
        syn::parse_quote!(Hash),
    ];
    apply_derives(&mut st_out.attrs, required);

    let ident = &st_out.ident;
    let generics = st_out.generics.clone();
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let out = quote! {
        #st_out

        impl #impl_generics #ident #ty_generics #where_clause {
            pub fn new(value: #inner_ty) -> Self { Self(value) }
        }

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

        impl #impl_generics ::core::convert::AsRef<#inner_ty> for #ident #ty_generics #where_clause {
            fn as_ref(&self) -> &#inner_ty { &self.0 }
        }

        impl #impl_generics ::core::convert::AsMut<#inner_ty> for #ident #ty_generics #where_clause {
            fn as_mut(&mut self) -> &mut #inner_ty { &mut self.0 }
        }

        impl #impl_generics ::core::convert::From<#ident #ty_generics> for #inner_ty #where_clause {
            fn from(value: #ident #ty_generics) -> Self { value.0 }
        }

        impl #impl_generics ::core::convert::From<&#ident #ty_generics> for #inner_ty #where_clause
        where #inner_ty: ::core::clone::Clone
        {
            fn from(value: &#ident #ty_generics) -> Self { value.0.clone() }
        }

        impl #impl_generics ::core::convert::From<#inner_ty> for #ident #ty_generics #where_clause {
            fn from(value: #inner_ty) -> Self { Self(value) }
        }

        impl #impl_generics ::core::convert::From<&#inner_ty> for #ident #ty_generics #where_clause
        where #inner_ty: ::core::clone::Clone
        {
            fn from(value: &#inner_ty) -> Self { Self(value.clone()) }
        }
    };

    TokenStream::from(out)
}
