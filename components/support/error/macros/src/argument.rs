/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use quote::ToTokens;
use syn::{spanned::Spanned, Meta};

pub(crate) fn parse(arguments: &syn::AttributeArgs) -> syn::Result<proc_macro2::TokenStream> {
    // stub
    if arguments.len() != 1 {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "Expected #[handle_error(ErrorName)]",
        ));
    }
    let argument = arguments.first().unwrap();
    match argument {
        syn::NestedMeta::Meta(meta) => match meta {
            Meta::Path(meta_path) => Ok(meta_path.to_token_stream()),
            _ => Err(syn::Error::new(
                meta.span(),
                "Expected #[handle_error(ErrorName)]",
            )),
        },
        _ => Err(syn::Error::new(
            argument.span(),
            "Expected #[handle_error(ErrorName)]",
        )),
    }
}
