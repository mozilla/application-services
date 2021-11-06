// /* This Source Code Form is subject to the terms of the Mozilla Public
//  * License, v. 2.0. If a copy of the MPL was not distributed with this
//  * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use heck::{CamelCase, MixedCase, ShoutySnakeCase};
use std::fmt::{self, Display};

/// Get the idiomatic Kotlin rendering of a class name (for enums, records, errors, etc).
pub fn class_name(nm: &dyn fmt::Display) -> String {
    nm.to_string().to_camel_case()
}

/// Get the idiomatic Kotlin rendering of a variable name.
pub fn var_name(nm: &dyn fmt::Display) -> String {
    nm.to_string().to_mixed_case()
}

/// Get the idiomatic Kotlin rendering of an individual enum variant.
pub fn enum_variant_name(nm: &dyn fmt::Display) -> String {
    nm.to_string().to_shouty_snake_case()
}

/// Surrounds a property name with quotes. It is assumed that property names do not need escaping.
pub fn quoted(prop: &dyn Display) -> String {
    format!("\"{}\"", prop)
}
