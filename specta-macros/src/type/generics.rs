use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, ToTokens};
use syn::{
    parse_quote, spanned::Spanned, ConstParam, Error, GenericArgument, GenericParam, Generics,
    Ident, LifetimeParam, PathArguments, Type, TypeArray, TypeParam, TypePtr, TypeReference,
    TypeSlice, WhereClause,
};

pub fn generics_with_ident_and_bounds_only(generics: &Generics) -> Option<TokenStream> {
    (!generics.params.is_empty())
        .then(|| {
            use GenericParam::*;
            generics.params.iter().map(|param| match param {
                Type(TypeParam {
                    ident,
                    colon_token,
                    bounds,
                    ..
                }) => quote!(#ident #colon_token #bounds),
                Lifetime(LifetimeParam {
                    lifetime,
                    colon_token,
                    bounds,
                    ..
                }) => quote!(#lifetime #colon_token #bounds),
                Const(ConstParam {
                    const_token,
                    ident,
                    colon_token,
                    ty,
                    ..
                }) => quote!(#const_token #ident #colon_token #ty),
            })
        })
        .map(|gs| quote!(<#(#gs),*>))
}

pub fn generics_with_ident_only(generics: &Generics) -> Option<TokenStream> {
    (!generics.params.is_empty())
        .then(|| {
            use GenericParam::*;

            generics.params.iter().map(|param| match param {
                Type(TypeParam { ident, .. }) | Const(ConstParam { ident, .. }) => quote!(#ident),
                Lifetime(LifetimeParam { lifetime, .. }) => quote!(#lifetime),
            })
        })
        .map(|gs| quote!(<#(#gs),*>))
}

// Code copied from ts-rs. Thanks to it's original author!
// generate start of the `impl #r#trait for #ty` block, up to (excluding) the open brace
pub fn impl_heading(r#trait: TokenStream, ty: &TokenStream, generics: &Generics) -> TokenStream {
    let bounds = generics_with_ident_and_bounds_only(generics);
    let type_args = generics_with_ident_only(generics);

    let where_bound = add_type_to_where_clause(&r#trait, generics);
    quote!(impl #bounds #r#trait for #ty #type_args #where_bound)
}

// Code copied from ts-rs. Thanks to it's original author!
pub fn add_type_to_where_clause(ty: &TokenStream, generics: &Generics) -> Option<WhereClause> {
    let generic_types = generics
        .params
        .iter()
        .filter_map(|gp| match gp {
            GenericParam::Type(ty) => Some(ty.ident.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    if generic_types.is_empty() {
        return generics.where_clause.clone();
    }
    match generics.where_clause {
        None => Some(parse_quote! { where #( #generic_types : #ty ),* }),
        Some(ref w) => {
            let bounds = w.predicates.iter();
            Some(parse_quote! { where #(#bounds,)* #( #generic_types : #ty ),* })
        }
    }
}

type DtGenericFn = fn(&TokenStream, TokenStream) -> TokenStream;
fn dt_generic_fn(f: DtGenericFn) -> DtGenericFn {
    f
}

pub fn construct_datatype(
    var_ident: Ident,
    ty: &Type,
    generic_idents: &[(usize, &Ident)],
    crate_ref: &TokenStream,
    inline: bool,
) -> syn::Result<TokenStream> {
    let (method, transform, generics) = match inline {
        true => (
            quote!(inline),
            quote!(),
            dt_generic_fn(|crate_ref, tokens| quote!(#crate_ref::Generics::Provided(#tokens))),
        ),
        false => (
            quote!(reference),
            quote!(.inner),
            dt_generic_fn(|_, tokens| tokens),
        ),
    };

    let path = match ty {
        Type::Tuple(t) => {
            let elems = t
                .elems
                .iter()
                .enumerate()
                .map(|(i, el)| {
                    construct_datatype(
                        format_ident!("{}_{}", var_ident, i),
                        el,
                        generic_idents,
                        crate_ref,
                        inline,
                    )
                })
                .collect::<syn::Result<Vec<TokenStream>>>()?;

            let generic_var_idents = t
                .elems
                .iter()
                .enumerate()
                .map(|(i, _)| format_ident!("{}_{}", &var_ident, i));

            let generics = generics(&crate_ref, quote!(&[#(#generic_var_idents),*]));
            return Ok(quote! {
                #(#elems)*

                let #var_ident = <#ty as #crate_ref::Type>::#method(type_map, #generics)#transform;
            });
        }
        Type::Paren(p) => {
            return construct_datatype(var_ident, &p.elem, generic_idents, crate_ref, inline)
        }
        Type::Array(TypeArray { elem, .. }) | Type::Slice(TypeSlice { elem, .. }) => {
            let elem_var_ident = format_ident!("{}_el", &var_ident);
            let elem = construct_datatype(
                elem_var_ident.clone(),
                elem,
                generic_idents,
                crate_ref,
                inline,
            )?;

            let generics = generics(&crate_ref, quote!(&[#elem_var_ident]));
            return Ok(quote! {
                #elem

                let #var_ident = <#ty as #crate_ref::Type>::#method(type_map, #generics)#transform;
            });
        }
        Type::Ptr(TypePtr { elem, .. }) | Type::Reference(TypeReference { elem, .. }) => {
            return construct_datatype(var_ident, elem, generic_idents, crate_ref, inline)
        }
        Type::Path(p) => &p.path,
        Type::TraitObject(_) => {
            return Err(syn::Error::new(
                ty.span(),
                "specta: trait objects are not currently supported.",
            ));
        }
        Type::Macro(m) => {
            let generics = generics(&crate_ref, quote!(&[]));
            return Ok(quote! {
                let #var_ident = <#m as #crate_ref::Type>::#method(type_map, #generics)#transform;
            });
        }
        ty => {
            return Err(syn::Error::new(
                ty.span(),
                format!(
                    "specta: Cannot get path from type `{}`",
                    ty.to_token_stream()
                ),
            ));
        }
    };

    if let Some(type_ident) = path.get_ident() {
        if let Some((i, generic_ident)) = generic_idents
            .iter()
            .find(|(_, ident)| ident == &type_ident)
        {
            let type_ident = type_ident.to_string();
            let generics = generics(
                &crate_ref,
                quote!(&[#crate_ref::DataType::Generic(std::borrow::Cow::Borrowed(#type_ident).into())]),
            );
            return Ok(quote! {
                let #var_ident = generics.get(#i).cloned().unwrap_or_else(
                    || {
                        <#generic_ident as #crate_ref::Type>::#method(type_map, #generics)#transform
                    },
                );
            });
        }
    }

    let generic_args = match &path.segments.last().unwrap().arguments {
        PathArguments::AngleBracketed(args) => args
            .args
            .iter()
            .enumerate()
            .filter_map(|(i, input)| match input {
                GenericArgument::Type(ty) => Some((i, ty)),
                _ => None,
            })
            .collect(),
        PathArguments::None => vec![],
        _ => {
            return Err(Error::new(
                Span::call_site(),
                "Only angle bracketed generics are supported!",
            ))
        }
    };

    let generic_vars = generic_args
        .iter()
        .map(|(i, path)| {
            construct_datatype(
                format_ident!("{}_{}", &var_ident, i),
                path,
                generic_idents,
                crate_ref,
                false,
            )
        })
        .collect::<syn::Result<Vec<TokenStream>>>()?;

    let generic_var_idents = generic_args
        .iter()
        .map(|(i, _)| format_ident!("{}_{}", &var_ident, i));

    let generics = generics(&crate_ref, quote!(&[#(#generic_var_idents),*]));
    Ok(quote! {
        #(#generic_vars)*

        let #var_ident = <#ty as #crate_ref::Type>::#method(type_map, #generics)#transform;
    })
}
