// /* This Source Code Form is subject to the terms of the Mozilla Public
//  * License, v. 2.0. If a copy of the MPL was not distributed with this
//  * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use super::{identifiers, ConcreteCodeOracle};
use std::fmt;

use crate::backends::{CodeOracle, TypeIdentifier};
use crate::intermediate_representation::Literal;

pub fn type_label(type_: &TypeIdentifier) -> Result<String, askama::Error> {
    let oracle = ConcreteCodeOracle;
    Ok(oracle.find(type_).type_label(&oracle))
}

pub fn _canonical_name(type_: &TypeIdentifier) -> Result<String, askama::Error> {
    let oracle = ConcreteCodeOracle;
    Ok(oracle.find(type_).canonical_name(&oracle))
}

pub fn literal(type_: &TypeIdentifier, literal: &Literal) -> Result<String, askama::Error> {
    let oracle = ConcreteCodeOracle;
    Ok(oracle.find(type_).literal(&oracle, literal))
}

pub fn get_value(
    type_: &TypeIdentifier,
    vars: &dyn fmt::Display,
    prop: &dyn fmt::Display,
) -> Result<String, askama::Error> {
    let oracle = ConcreteCodeOracle;
    Ok(oracle.find(type_).get_value(&oracle, vars, prop))
}

pub fn with_fallback(
    type_: &TypeIdentifier,
    overrides: &dyn fmt::Display,
    default: &dyn fmt::Display,
) -> Result<String, askama::Error> {
    let oracle = ConcreteCodeOracle;
    Ok(oracle
        .find(type_)
        .with_fallback(&oracle, overrides, default))
}

/// Get the idiomatic Kotlin rendering of a class name (for enums, records, errors, etc).
pub fn class_name(nm: &dyn fmt::Display) -> Result<String, askama::Error> {
    Ok(identifiers::class_name(nm))
}

/// Get the idiomatic Kotlin rendering of a variable name.
pub fn var_name(nm: &dyn fmt::Display) -> Result<String, askama::Error> {
    Ok(identifiers::var_name(nm))
}

/// Get the idiomatic Kotlin rendering of an individual enum variant.
pub fn enum_variant_name(nm: &dyn fmt::Display) -> Result<String, askama::Error> {
    Ok(identifiers::enum_variant_name(nm))
}

pub fn comment(txt: &dyn fmt::Display, spaces: &str) -> Result<String, askama::Error> {
    use textwrap::{fill, Options};

    let indent1 = "/** ".to_string();
    let indent2 = format!("{} * ", spaces);
    let indent3 = format!("{} */", spaces);

    let options = Options::new(80)
        .initial_indent(&indent1)
        .subsequent_indent(&indent2);

    let lines = fill(txt.to_string().as_str(), &options);
    Ok(format!(
        "{lines}\n{indent}",
        lines = lines,
        indent = indent3
    ))
}

pub fn quoted(txt: &dyn fmt::Display) -> Result<String, askama::Error> {
    Ok(identifiers::quoted(txt))
}
