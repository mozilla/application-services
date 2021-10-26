// /* This Source Code Form is subject to the terms of the Mozilla Public
//  * License, v. 2.0. If a copy of the MPL was not distributed with this
//  * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use std::fmt::{self, Display};

use crate::backends::{CodeOracle, TypeIdentifier};
use crate::intermediate_representation::Literal;

pub fn type_label(
    oracle: &impl CodeOracle,
    type_: &TypeIdentifier,
) -> Result<String, askama::Error> {
    Ok(oracle.find(type_).type_label(oracle))
}

pub fn canonical_name(
    oracle: &impl CodeOracle,
    type_: &TypeIdentifier,
) -> Result<String, askama::Error> {
    Ok(oracle.find(type_).canonical_name(oracle))
}

pub fn literal(
    oracle: &impl CodeOracle,
    literal: &Literal,
    type_: &TypeIdentifier,
) -> Result<String, askama::Error> {
    Ok(oracle.find(type_).literal(oracle, literal))
}

/// Get the idiomatic Kotlin rendering of a class name (for enums, records, errors, etc).
pub fn class_name(
    oracle: &impl CodeOracle,
    nm: &dyn fmt::Display,
) -> Result<String, askama::Error> {
    Ok(oracle.class_name(nm))
}

/// Get the idiomatic Kotlin rendering of a function name.
pub fn fn_name(oracle: &impl CodeOracle, nm: &dyn fmt::Display) -> Result<String, askama::Error> {
    Ok(oracle.fn_name(nm))
}

/// Get the idiomatic Kotlin rendering of a variable name.
pub fn var_name(oracle: &impl CodeOracle, nm: &dyn fmt::Display) -> Result<String, askama::Error> {
    Ok(oracle.var_name(nm))
}

/// Get the idiomatic Kotlin rendering of an individual enum variant.
pub fn enum_variant(
    oracle: &impl CodeOracle,
    nm: &dyn fmt::Display,
) -> Result<String, askama::Error> {
    Ok(oracle.enum_variant_name(nm))
}

/// Get the idiomatic Kotlin rendering of an exception name
///
/// This replaces "Error" at the end of the name with "Exception".  Rust code typically uses
/// "Error" for any type of error but in the Java world, "Error" means a non-recoverable error
/// and is distinguished from an "Exception".
pub fn exception_name(
    oracle: &impl CodeOracle,
    nm: &dyn fmt::Display,
) -> Result<String, askama::Error> {
    Ok(oracle.error_name(nm))
}

/// Surrounds a property name with quotes. It is assumed that property names do not need escaping.
pub fn quoted(prop: &dyn Display) -> String {
    format!("\"{}\"", prop)
}
