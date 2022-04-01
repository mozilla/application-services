// /* This Source Code Form is subject to the terms of the Mozilla Public
//  * License, v. 2.0. If a copy of the MPL was not distributed with this
//  * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use super::{common, ConcreteCodeOracle};
use std::fmt::{self, Display};

use crate::backends::{CodeOracle, LiteralRenderer, TypeIdentifier};
use crate::intermediate_representation::Literal;

pub fn type_label(type_: &TypeIdentifier) -> Result<String, askama::Error> {
    let oracle = ConcreteCodeOracle;
    Ok(oracle.find(type_).type_label(&oracle))
}

pub fn defaults_type_label(type_: &TypeIdentifier) -> Result<String, askama::Error> {
    let oracle = ConcreteCodeOracle;
    Ok(oracle.find(type_).defaults_type(&oracle))
}

pub fn literal(
    type_: &TypeIdentifier,
    renderer: &dyn LiteralRenderer,
    literal: &Literal,
    ctx: &dyn Display,
) -> Result<String, askama::Error> {
    let oracle = ConcreteCodeOracle;
    Ok(oracle.find(type_).literal(&oracle, ctx, renderer, literal))
}

pub fn property(
    type_: &TypeIdentifier,
    prop: &dyn fmt::Display,
    vars: &dyn fmt::Display,
    default: &dyn fmt::Display,
) -> Result<String, askama::Error> {
    let oracle = &ConcreteCodeOracle;
    let ct = oracle.find(type_);
    Ok(ct.property_getter(oracle, vars, prop, default))
}

/// Get the idiomatic Kotlin rendering of a class name (for enums, records, errors, etc).
pub fn class_name(nm: &dyn fmt::Display) -> Result<String, askama::Error> {
    Ok(common::class_name(nm))
}

/// Get the idiomatic Kotlin rendering of a variable name.
pub fn var_name(nm: &dyn fmt::Display) -> Result<String, askama::Error> {
    Ok(common::var_name(nm))
}

/// Get the idiomatic Kotlin rendering of an individual enum variant.
pub fn enum_variant_name(nm: &dyn fmt::Display) -> Result<String, askama::Error> {
    Ok(common::enum_variant_name(nm))
}

pub fn comment(txt: &dyn fmt::Display, spaces: &str) -> Result<String, askama::Error> {
    use textwrap::{fill, Options};

    let indent_start = "/** ".to_string();
    let indent_mid = format!("{} * ", spaces);
    let indent_end = format!("{} */", spaces);

    let options = Options::new(80)
        .initial_indent(&indent_mid)
        .subsequent_indent(&indent_mid);

    let lines = fill(txt.to_string().as_str(), &options);
    Ok(format!(
        "{start}\n{lines}\n{indent}",
        start = indent_start,
        lines = lines,
        indent = indent_end
    ))
}

pub fn quoted(txt: &dyn fmt::Display) -> Result<String, askama::Error> {
    Ok(common::quoted(txt))
}
