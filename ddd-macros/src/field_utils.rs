use syn::{Field, FieldsNamed, Token, Type, punctuated::Punctuated};

fn has_field_named_in(named: &Punctuated<Field, Token![,]>, name: &str) -> bool {
    named
        .iter()
        .any(|f| f.ident.as_ref().map(|i| i == name).unwrap_or(false))
}

/// 确保具名字段结构体/变体包含所需字段
/// - required: (字段名, 字段类型) 列表，按给定顺序处理
/// - reposition_existing: 若为 true，则即使已存在也会把所需字段移至最前（entity 需要）；
///   若为 false，则仅在缺失时追加，保留既有顺序（event 需要）。
pub(crate) fn ensure_required_fields(
    fields_named: &mut FieldsNamed,
    required: &[(&str, &Type)],
    reposition_existing: bool,
) {
    let old_named = fields_named.named.clone();
    let mut new_named: Punctuated<Field, Token![,]> = Punctuated::new();

    if reposition_existing {
        // 把需要的字段按顺序放在前面：若存在则复用原字段；否则创建新字段
        for (name, ty) in required.iter() {
            if let Some(existing) = old_named
                .iter()
                .find(|f| f.ident.as_ref().map(|i| i == *name).unwrap_or(false))
            {
                new_named.push(existing.clone());
            } else {
                let ident: syn::Ident = syn::parse_str(name).expect("valid field ident");
                let field: Field = syn::parse_quote! { #ident: #ty };
                new_named.push(field);
            }
        }

        // 其余非必需字段保持原始顺序
        for f in old_named.into_iter() {
            let is_required = f
                .ident
                .as_ref()
                .map(|i| required.iter().any(|(n, _)| i == n))
                .unwrap_or(false);
            if !is_required {
                new_named.push(f);
            }
        }
    } else {
        // 仅在缺失时追加（位于最前），否则保留原始顺序
        for (name, ty) in required.iter() {
            if !has_field_named_in(&old_named, name) {
                let ident: syn::Ident = syn::parse_str(name).expect("valid field ident");
                let field: Field = syn::parse_quote! { #ident: #ty };
                new_named.push(field);
            }
        }
        for f in old_named.into_iter() {
            new_named.push(f);
        }
    }

    fields_named.named = new_named;
}
