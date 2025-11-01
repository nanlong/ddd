use crate::utils::apply_derives;
use proc_macro::TokenStream;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{Item, Result, Token, parse::Parse, parse::ParseStream, parse_macro_input};

/// #[value_object] 宏实现
/// - 支持结构体（具名或 tuple）与枚举
/// - 合并/追加派生：Default, Clone, (Debug 可控), Serialize, Deserialize, PartialEq, Eq
/// - 参数：`#[value_object(debug = true|false)]`，默认 true
pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    let cfg = parse_macro_input!(attr as ValueObjectAttrConfig);
    let mut input = parse_macro_input!(item as Item);

    // 组装需要的 derive 集合（struct/enum 通用）
    let mut required: Vec<syn::Path> = vec![
        syn::parse_quote!(Default),
        syn::parse_quote!(Clone),
        syn::parse_quote!(serde::Serialize),
        syn::parse_quote!(serde::Deserialize),
        syn::parse_quote!(PartialEq),
        syn::parse_quote!(Eq),
    ];

    if cfg.derive_debug.unwrap_or(true) {
        required.insert(0, syn::parse_quote!(Debug));
    }

    match &mut input {
        Item::Struct(st) => {
            apply_derives(&mut st.attrs, required);
            TokenStream::from(quote! { #st })
        }
        Item::Enum(en) => {
            apply_derives(&mut en.attrs, required);
            TokenStream::from(quote! { #en })
        }
        other => syn::Error::new(other.span(), "#[value_object] only supports struct or enum")
            .to_compile_error()
            .into(),
    }
}

// -------- parsing --------

struct ValueObjectAttrConfig {
    derive_debug: Option<bool>,
}

impl Parse for ValueObjectAttrConfig {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.is_empty() {
            return Ok(Self { derive_debug: None });
        }

        let mut derive_debug: Option<bool> = None;
        let pairs: Punctuated<ValueObjectAttrElem, Token![,]> =
            Punctuated::parse_terminated(input)?;

        for elem in pairs {
            match elem {
                ValueObjectAttrElem::Debug(b) => {
                    if derive_debug.is_some() {
                        return Err(syn::Error::new(
                            proc_macro2::Span::call_site(),
                            "duplicate key 'debug' in attribute",
                        ));
                    }
                    derive_debug = Some(b);
                }
            }
        }
        Ok(Self { derive_debug })
    }
}

enum ValueObjectAttrElem {
    Debug(bool),
}

impl Parse for ValueObjectAttrElem {
    fn parse(input: ParseStream) -> Result<Self> {
        let key: syn::Ident = input.parse()?;
        if key == "debug" {
            let _eq: Token![=] = input.parse()?;
            let expr: syn::Expr = input.parse()?;
            match expr {
                syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Bool(b),
                    ..
                }) => Ok(Self::Debug(b.value())),
                other => Err(syn::Error::new(
                    other.span(),
                    "expected boolean literal for 'debug'",
                )),
            }
        } else {
            Err(syn::Error::new(
                key.span(),
                "unknown key in attribute; expected 'debug'",
            ))
        }
    }
}
