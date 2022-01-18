// /* This Source Code Form is subject to the terms of the Mozilla Public
//  * License, v. 2.0. If a copy of the MPL was not distributed with this
//  * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use heck::{CamelCase, MixedCase, ShoutySnakeCase};
use std::fmt::Display;

/// Get the idiomatic Swift rendering of a class name (for enums, records, errors, etc).
pub fn class_name(nm: &dyn Display) -> String {
    nm.to_string().to_camel_case()
}

/// Get the idiomatic Swift rendering of a variable name.
pub fn var_name(nm: &dyn Display) -> String {
    nm.to_string().to_mixed_case()
}

/// Get the idiomatic Swift rendering of an individual enum variant.
pub fn enum_variant_name(nm: &dyn Display) -> String {
    nm.to_string().to_mixed_case()
}

/// Surrounds a property name with quotes. It is assumed that property names do not need escaping.
pub fn quoted(prop: &dyn Display) -> String {
    format!("\"{}\"", prop)
}

pub(crate) mod code_type {
    use std::fmt::Display;

    use crate::backends::{CodeOracle, CodeType};

    /// The language specific expression that gets a value of the `prop` from the `vars` object.
    pub(crate) fn property_getter(
        ct: &dyn CodeType,
        oracle: &dyn CodeOracle,
        vars: &dyn Display,
        prop: &dyn Display,
        default: &dyn Display,
    ) -> String {
        let getter = ct.value_getter(oracle, vars, prop);

        let getter = if let Some(mapper) = ct.value_mapper(oracle) {
            format!("{getter}?.{mapper}", getter = getter, mapper = mapper)
        } else {
            getter
        };

        let getter = if let Some(merger) = ct.value_merger(oracle, default) {
            format!("{getter}?.{merger}", getter = getter, merger = merger)
        } else {
            getter
        };

        format!(
            "{getter} ?? {fallback}",
            getter = getter,
            fallback = default
        )
    }

    pub(crate) fn value_getter(
        ct: &dyn CodeType,
        oracle: &dyn CodeOracle,
        vars: &dyn Display,
        prop: &dyn Display,
    ) -> String {
        let vt = ct.variables_type(oracle);
        format!(
            "{vars}?.get{vt}(\"{prop}\")",
            vars = vars,
            vt = vt,
            prop = prop
        )
    }

    pub(crate) fn value_mapper(ct: &dyn CodeType, oracle: &dyn CodeOracle) -> Option<String> {
        let transform = ct.create_transform(oracle)?;
        Some(format!("let({})", transform))
    }
}
