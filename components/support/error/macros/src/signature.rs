/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use syn::{spanned::Spanned, GenericArgument, PathArguments, Type};

pub(crate) fn parse(sig: &syn::Signature) -> syn::Result<(Type, Type)> {
    if let syn::ReturnType::Type(_, typ) = &sig.output {
        let original_ret_typ = typ.clone();
        if let syn::Type::Path(type_path) = typ.as_ref() {
            let seg = type_path.path.segments.last().ok_or_else(|| {
                syn::Error::new(type_path.span(), "Expected a Result<T> or Result<T, E>")
            })?;
            match &seg.arguments {
                PathArguments::AngleBracketed(generic_args) => {
                    let generic_arg = generic_args.args.first().ok_or_else(|| {
                        syn::Error::new(generic_args.span(), "Expected a Result<T> or Result<T, E>")
                    })?;
                    if let GenericArgument::Type(t) = generic_arg {
                        Ok((t.clone(), *original_ret_typ))
                    } else {
                        Err(syn::Error::new(
                            generic_arg.span(),
                            "Expected a Result<T> or Result<T, E>",
                        ))
                    }
                }
                _ => Err(syn::Error::new(
                    seg.span(),
                    "Expected a Result<T> or Result<T, E>",
                )),
            }
        } else {
            Err(syn::Error::new(
                typ.span(),
                "Expected a Result<T> or Result<T, E>",
            ))
        }
    } else {
        Err(syn::Error::new(
            sig.output.span(),
            "Expected a Result<T> or Result<T, E>",
        ))
    }
}
