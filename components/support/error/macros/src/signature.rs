/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use syn::{spanned::Spanned, GenericArgument, PathArguments};

const WRONG_RETURN_TYPE_MSG: &str = "Expected Result<T> or Result<T, E>";

pub(crate) fn validate(sig: &syn::Signature) -> syn::Result<()> {
    if let syn::ReturnType::Type(_, typ) = &sig.output {
        if let syn::Type::Path(type_path) = typ.as_ref() {
            let seg = type_path
                .path
                .segments
                .last()
                .ok_or_else(|| syn::Error::new(type_path.span(), WRONG_RETURN_TYPE_MSG))?;
            match &seg.arguments {
                PathArguments::AngleBracketed(generic_args) => {
                    let generic_arg = generic_args.args.first().ok_or_else(|| {
                        syn::Error::new(generic_args.span(), WRONG_RETURN_TYPE_MSG)
                    })?;
                    if let GenericArgument::Type(_) = generic_arg {
                        Ok(())
                    } else {
                        Err(syn::Error::new(generic_arg.span(), WRONG_RETURN_TYPE_MSG))
                    }
                }
                _ => Err(syn::Error::new(seg.span(), WRONG_RETURN_TYPE_MSG)),
            }
        } else {
            Err(syn::Error::new(typ.span(), WRONG_RETURN_TYPE_MSG))
        }
    } else {
        Err(syn::Error::new(sig.output.span(), WRONG_RETURN_TYPE_MSG))
    }
}
