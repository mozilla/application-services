/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_quote, spanned::Spanned};

mod argument;
mod signature;

#[proc_macro_attribute]
pub fn handle_error(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = syn::parse_macro_input!(args as syn::AttributeArgs);
    let parsed = syn::parse_macro_input!(input as syn::Item);
    TokenStream::from(match impl_handle_error(&parsed, &args) {
        Ok(res) => res,
        Err(e) => e.to_compile_error(),
    })
}

fn impl_handle_error(
    input: &syn::Item,
    arguments: &syn::AttributeArgs,
) -> syn::Result<proc_macro2::TokenStream> {
    let argument_name = argument::parse(arguments)?;
    if let syn::Item::Fn(item_fn) = input {
        let (ok_type, original_ret_typ) = signature::parse(&item_fn.sig)?;
        let original_body = &item_fn.block;

        let mut new_fn = item_fn.clone();
        new_fn.block = parse_quote! {
            {
                (|| -> #original_ret_typ {
                    #original_body
                })().map_err(::error_support::convert_log_report_error)
            }
        };
        new_fn.sig.output = parse_quote!(-> ::std::result::Result<#ok_type, #argument_name>);

        Ok(quote! {
            #new_fn
        })
    } else {
        Err(syn::Error::new(
            input.span(),
            "#[handle_error] can only be used on functions",
        ))
    }
}
