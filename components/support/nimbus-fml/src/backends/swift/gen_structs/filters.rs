// /* This Source Code Form is subject to the terms of the Mozilla Public
//  * License, v. 2.0. If a copy of the MPL was not distributed with this
//  * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use super::{common, ConcreteCodeOracle};
use std::borrow::Borrow;
use std::fmt::{self, Display};

use crate::backends::{CodeOracle, LiteralRenderer, TypeIdentifier};
use crate::intermediate_representation::Literal;

pub fn type_label(type_: impl Borrow<TypeIdentifier>) -> Result<String, askama::Error> {
    let oracle = ConcreteCodeOracle;
    Ok(oracle.find(type_.borrow()).type_label(&oracle))
}

pub fn defaults_type_label(type_: impl Borrow<TypeIdentifier>) -> Result<String, askama::Error> {
    let oracle = ConcreteCodeOracle;
    Ok(oracle.find(type_.borrow()).defaults_type(&oracle))
}

pub fn literal(
    type_: impl Borrow<TypeIdentifier>,
    renderer: impl LiteralRenderer,
    literal: impl Borrow<Literal>,
    ctx: impl Display,
) -> Result<String, askama::Error> {
    let oracle = ConcreteCodeOracle;
    Ok(oracle
        .find(type_.borrow())
        .literal(&oracle, &ctx, &renderer, literal.borrow()))
}

pub fn property(
    type_: impl Borrow<TypeIdentifier>,
    prop: impl fmt::Display,
    vars: impl fmt::Display,
    default: impl fmt::Display,
) -> Result<String, askama::Error> {
    let oracle = &ConcreteCodeOracle;
    let ct = oracle.find(type_.borrow());
    Ok(ct.property_getter(oracle, &vars, &prop, &default))
}

/// Get the idiomatic Swift rendering of a class name (for enums, records, errors, etc).
pub fn class_name(nm: impl fmt::Display) -> Result<String, askama::Error> {
    Ok(common::class_name(&nm))
}

/// Get the idiomatic Swift rendering of a variable name.
pub fn var_name(nm: impl fmt::Display) -> Result<String, askama::Error> {
    Ok(common::var_name(&nm))
}

/// Get the idiomatic Swift rendering of an individual enum variant.
pub fn enum_variant_name(nm: impl fmt::Display) -> Result<String, askama::Error> {
    Ok(common::enum_variant_name(&nm))
}

pub fn comment(txt: impl fmt::Display, spaces: &str) -> Result<String, askama::Error> {
    use textwrap::{fill, Options};

    let indent1 = "/// ".to_string();
    let indent2 = format!("{} /// ", spaces);

    let options = Options::new(80)
        .initial_indent(&indent1)
        .subsequent_indent(&indent2);

    let lines = fill(txt.to_string().as_str(), options);
    Ok(lines)
}

pub fn quoted(txt: impl fmt::Display) -> Result<String, askama::Error> {
    Ok(common::quoted(&txt))
}
