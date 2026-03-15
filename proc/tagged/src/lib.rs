#![no_std]

use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, Attribute, Data, DeriveInput, Error, Field, Fields,
    Meta, Type, Visibility,
};

#[proc_macro_derive(Tagged, attributes(tagged))]
pub fn derive_tagged_priv(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match derive_tagged_priv_inner(input) {
        Ok(ts) => ts,
        Err(e) => e.to_compile_error().into(),
    }
}

fn derive_tagged_priv_inner(input: DeriveInput) -> Result<TokenStream, Error> {
    /* Must be a structure */
    let data = match &input.data {
        Data::Struct(s) => s,
        _ => {
            return Err(Error::new_spanned(
                input,
                "Tagged can only be derived for structs",
            ))
        }
    };

    /* Must be #[repr(C)] to guarantee field offsets */
    if !has_repr_c(&input.attrs) {
        return Err(Error::new_spanned(
            &input,
            "Struct must be #[repr(C)] to guarantee the tag field is at offset 0",
        ));
    }

    /* Must have named fields */
    let fields = match &data.fields {
        Fields::Named(f) => &f.named,
        _ => {
            return Err(Error::new_spanned(
                &data.fields,
                "Tagged requires a struct with named fields",
            ))
        }
    };

    /* Must have at least one field */
    if fields.is_empty() {
        return Err(Error::new_spanned(
            &data.fields,
            "Struct must have at least one field (the tag)",
        ));
    }

    /* Check the first field */
    let first_field = fields.iter().next().unwrap();
    check_first_field(first_field)?;

    /* Extract the tag expression from #[tagged(...)] */
    let tagged_attr = input
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("tagged"))
        .ok_or_else(|| {
            Error::new_spanned(&input, "Missing #[tagged(...)] attribute")
        })?;
    let tag_expr = tagged_attr.parse_args::<syn::Expr>()?;

    let name = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let expanded = quote! {
        const _: () = assert!(core::mem::offset_of!(#name, tag) == 0);
        unsafe impl #impl_generics Tagged for #name #ty_generics #where_clause {
            const TAG: u64 = #tag_expr;
        }
    };

    Ok(expanded.into())
}

/// Returns true if the attribute list contains `#[repr(C)]`.
fn has_repr_c(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("repr") {
            return false;
        }
        // Parse the meta inside #[repr(...)].
        match &attr.meta {
            Meta::List(list) => {
                // Nested meta items are separated by commas.
                use syn::punctuated::Punctuated;
                use syn::token::Comma;
                let nested: Result<Punctuated<Meta, Comma>, _> =
                    list.parse_args_with(Punctuated::parse_terminated);
                if let Ok(items) = nested {
                    items.iter().any(|item| {
                        // Look for a plain `C` identifier.
                        matches!(item, Meta::Path(p) if p.is_ident("C"))
                    })
                } else {
                    false
                }
            }
            _ => false,
        }
    })
}

/// Checks that a field is `pub tag: u64`.
fn check_first_field(field: &Field) -> Result<(), Error> {
    // Visibility must be public.
    if !matches!(&field.vis, Visibility::Public(_)) {
        return Err(Error::new_spanned(
            &field.vis,
            "The first field must be public (`pub tag: u64`)",
        ));
    }

    // Field name must be "tag".
    let name = field
        .ident
        .as_ref()
        .ok_or_else(|| Error::new_spanned(field, "Expected named field"))?;
    if name != "tag" {
        return Err(Error::new_spanned(
            name,
            "The first field must be named `tag`",
        ));
    }

    // Type must be `u64` (exactly).
    match &field.ty {
        Type::Path(ty_path) if ty_path.qself.is_none() => {
            let segments = &ty_path.path.segments;
            if segments.len() != 1 || segments[0].ident != "u64" {
                return Err(Error::new_spanned(
                    &field.ty,
                    "The first field must have type `u64`",
                ));
            }
            // Ensure no generic arguments.
            if !segments[0].arguments.is_empty() {
                return Err(Error::new_spanned(
                    &field.ty,
                    "The first field must have type `u64` (no generics)",
                ));
            }
        }
        _ => {
            return Err(Error::new_spanned(
                &field.ty,
                "The first field must have type `u64`",
            ));
        }
    }

    Ok(())
}
