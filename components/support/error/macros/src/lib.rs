/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_quote, spanned::Spanned, GenericArgument, PathArguments};

mod argument;

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

    let (mut sig, body, vis) = if let syn::Item::Fn(item_fn) = input {
        (
            item_fn.sig.clone(),
            item_fn.block.clone(),
            item_fn.vis.clone(),
        )
    } else {
        return Err(syn::Error::new(
            input.span(),
            "The macro should only be used on functions",
        ));
    };

    let output = sig.output.clone();
    let ret_output;
    let ok_type = if let syn::ReturnType::Type(_, typ) = output {
        ret_output = typ.clone();
        if let syn::Type::Path(type_path) = *typ {
            let seg = type_path.path.segments.last().ok_or_else(|| {
                syn::Error::new(type_path.span(), "Expected a Result<T> or Result<T, E>")
            })?;
            match &seg.arguments {
                PathArguments::AngleBracketed(generic_args) => {
                    let generic_arg = generic_args.args.first().ok_or_else(|| {
                        syn::Error::new(generic_args.span(), "Expected a Result<T> or Result<T, E>")
                    })?;
                    if let GenericArgument::Type(t) = generic_arg {
                        t.clone()
                    } else {
                        return Err(syn::Error::new(
                            generic_arg.span(),
                            "Expected a Result<T> or Result<T, E>",
                        ));
                    }
                }
                _ => {
                    return Err(syn::Error::new(
                        seg.span(),
                        "Expected a Result<T> or Result<T, E>",
                    ))
                }
            }
        } else {
            return Err(syn::Error::new(
                typ.span(),
                "Expected a Result<T> or Result<T, E>",
            ));
        }
    } else {
        return Err(syn::Error::new(
            output.span(),
            "Expected a Result<T> or Result<T, E>",
        ));
    };

    sig.output = parse_quote!(-> ::std::result::Result<#ok_type, #argument_name>);

    Ok(quote! {
        #vis #sig {
        (|| -> #ret_output {
                #body
        })().map_err(::error_support::convert_log_report_error)
        }
    })
}
