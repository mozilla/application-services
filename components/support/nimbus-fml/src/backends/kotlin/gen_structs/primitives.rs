/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::fmt::Display;

use super::filters;
use crate::{
    backends::{CodeOracle, CodeType},
    intermediate_representation::Literal,
};
pub(crate) struct BooleanCodeType;

impl CodeType for BooleanCodeType {
    /// The language specific label used to reference this type. This will be used in
    /// method signatures and property declarations.
    fn type_label(&self, oracle: &dyn CodeOracle) -> String {
        "Boolean".into()
    }

    /// The language specific expression that gets a value of the `prop` from the `vars` object.
    fn get_value(&self, oracle: &dyn CodeOracle, vars: &dyn Display, prop: &dyn Display) -> String {
        format!("{}.getBool({})", vars, filters::quoted(prop))
    }

    /// A representation of the given literal for this type.
    /// N.B. `Literal` is aliased from `interface::Literal`, so may not be whole suited to this task.
    fn literal(&self, _oracle: &dyn CodeOracle, literal: &Literal) -> String {
        match literal {
            serde_json::Value::Bool(v) => {
                if *v {
                    "true".to_string()
                } else {
                    "false".to_string()
                }
            }
            _ => unreachable!(),
        }
    }
}

pub(crate) struct IntCodeType;

impl CodeType for IntCodeType {
    /// The language specific label used to reference this type. This will be used in
    /// method signatures and property declarations.
    fn type_label(&self, oracle: &dyn CodeOracle) -> String {
        "Int".into()
    }

    /// The language specific expression that gets a value of the `prop` from the `vars` object.
    fn get_value(&self, oracle: &dyn CodeOracle, vars: &dyn Display, prop: &dyn Display) -> String {
        format!("{}.getInt({})", vars, filters::quoted(prop))
    }

    /// A representation of the given literal for this type.
    /// N.B. `Literal` is aliased from `interface::Literal`, so may not be whole suited to this task.
    fn literal(&self, _oracle: &dyn CodeOracle, literal: &Literal) -> String {
        match literal {
            serde_json::Value::Number(v) => {
                format!("{:.0}", v)
            }
            _ => unreachable!(),
        }
    }
}

pub(crate) struct StringCodeType;

impl CodeType for StringCodeType {
    /// The language specific label used to reference this type. This will be used in
    /// method signatures and property declarations.
    fn type_label(&self, _oracle: &dyn CodeOracle) -> String {
        "String".into()
    }

    /// The language specific expression that gets a value of the `prop` from the `vars` object.
    fn get_value(
        &self,
        _oracle: &dyn CodeOracle,
        vars: &dyn Display,
        prop: &dyn Display,
    ) -> String {
        format!("{}.getString({})", vars, filters::quoted(prop))
    }

    /// A representation of the given literal for this type.
    /// N.B. `Literal` is aliased from `interface::Literal`, so may not be whole suited to this task.
    fn literal(&self, _oracle: &dyn CodeOracle, literal: &Literal) -> String {
        match literal {
            serde_json::Value::String(v) => {
                format!("\"{0}\"", v)
            }
            _ => unreachable!(),
        }
    }
}
