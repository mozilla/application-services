/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Compile-time URL validation macro for application-services.
//!
//! Provides a [`url!`] proc macro that validates a string literal as a
//! parseable URL at compile time. Invalid input produces a compile
//! error pointing at the offending literal; valid input expands to a
//! runtime `url::Url::parse` call that is guaranteed to succeed.
//!
//! ## Consumer requirements
//!
//! The expanded code references `::url::Url::parse`, so the calling
//! crate must declare `url` as a dependency.
//!
//! ## Limitations
//!
//! `url::Url::parse` is not `const fn`, so the macro cannot be used
//! directly in `static`/`const` initializers. Wrap with
//! `once_cell::Lazy` or `std::sync::LazyLock` for static contexts.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, LitStr};

/// Validates a URL string literal at compile time.
///
/// On success, expands to a `url::Url::parse(...)` call that cannot
/// fail at runtime. On failure, the macro emits a compile error
/// carrying `url::ParseError`'s message, anchored at the offending
/// literal.
///
/// # Examples
///
/// ```ignore
/// use url_macro::url;
/// use url::Url;
///
/// let endpoint: Url = url!("https://ads.mozilla.org/v1/");
/// // url!("not a url"); // -> compile error: relative URL without a base
/// ```
#[proc_macro]
pub fn url(input: TokenStream) -> TokenStream {
    let lit = parse_macro_input!(input as LitStr);
    let value = lit.value();

    if let Err(err) = ::url::Url::parse(&value) {
        return syn::Error::new(lit.span(), err.to_string())
            .to_compile_error()
            .into();
    }

    quote! {
        ::url::Url::parse(#lit).expect("URL validated at compile time")
    }
    .into()
}
