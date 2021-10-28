/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::fmt::Display;

use super::identifiers;
use crate::backends::VariablesType;
use crate::{
    backends::{CodeOracle, CodeType, TypeIdentifier},
    intermediate_representation::Literal,
};

pub(crate) struct MapCodeType {
    k_type: TypeIdentifier,
    v_type: TypeIdentifier,
}

impl MapCodeType {
    pub(crate) fn new(k: &TypeIdentifier, v: &TypeIdentifier) -> Self {
        Self {
            k_type: k.clone(),
            v_type: v.clone(),
        }
    }
}

impl CodeType for MapCodeType {
    /// The language specific label used to reference this type. This will be used in
    /// method signatures and property declarations.
    fn type_label(&self, oracle: &dyn CodeOracle) -> String {
        format!(
            "Map<{k}, {v}>",
            k = oracle.find(&self.k_type).type_label(oracle),
            v = oracle.find(&self.v_type).type_label(oracle),
        )
    }

    /// The language specific expression that gets a value of the `prop` from the `vars` object.
    fn get_value(&self, oracle: &dyn CodeOracle, vars: &dyn Display, prop: &dyn Display) -> String {
        let k_type = oracle.find(&self.k_type);
        let v_type = oracle.find(&self.v_type);

        let getter = format!(
            "{vars}.get{vt}Map({prop})",
            vars = vars,
            vt = v_type.variables_type(oracle),
            prop = identifiers::quoted(prop),
        );

        let mapper = match (k_type.transform(oracle), v_type.transform(oracle)) {
            (Some(k), Some(v)) => format!("?.mapEntries({k}, {v})", k = k, v = v),
            (None, Some(v)) => format!("?.mapValues({v})", v = v),
            (Some(k), None) => format!("?.mapKeys({k})", k = k),
            _ => "".into(),
        };

        format!("{}{}", getter, mapper)
    }

    /// Accepts two runtime expressions and returns a runtime experession to combine. If the `default` is of type `T`,
    /// the `override` is of type `T?`.
    fn with_fallback(
        &self,
        _oracle: &dyn CodeOracle,
        overrides: &dyn Display,
        default: &dyn Display,
    ) -> String {
        format!(
            "{overrides}?.let {{ overrides -> {default} + overrides }} ?: {default}",
            overrides = overrides,
            default = default
        )
    }

    /// The name of the type as it's represented in the `Variables` object.
    /// The string return may be used to combine with an indentifier, e.g. a `Variables` method name.
    fn variables_type(&self, _oracle: &dyn CodeOracle) -> VariablesType {
        unimplemented!("Nesting maps in to lists and maps are not supported")
    }

    /// A representation of the given literal for this type.
    /// N.B. `Literal` is aliased from `interface::Literal`, so may not be whole suited to this task.
    fn literal(&self, oracle: &dyn CodeOracle, literal: &Literal) -> String {
        let variant = match literal {
            serde_json::Value::Object(v) => v,
            _ => unreachable!(),
        };

        let k_type = oracle.find(&self.k_type);
        let v_type = oracle.find(&self.v_type);
        let src: Vec<String> = variant
            .iter()
            .map(|(k, v)| {
                format!(
                    "{k} to {v}",
                    k = k_type.literal(oracle, &Literal::String(k.clone())),
                    v = v_type.literal(oracle, v)
                )
            })
            .collect();

        format!("mapOf({})", src.join(", "))
    }
}

// List type

pub(crate) struct ListCodeType {
    inner: TypeIdentifier,
}

impl ListCodeType {
    pub(crate) fn new(inner: &TypeIdentifier) -> Self {
        Self {
            inner: inner.clone(),
        }
    }
}

impl CodeType for ListCodeType {
    /// The language specific label used to reference this type. This will be used in
    /// method signatures and property declarations.
    fn type_label(&self, oracle: &dyn CodeOracle) -> String {
        format!(
            "List<{item}>",
            item = oracle.find(&self.inner).type_label(oracle),
        )
    }

    /// The language specific expression that gets a value of the `prop` from the `vars` object.
    fn get_value(&self, oracle: &dyn CodeOracle, vars: &dyn Display, prop: &dyn Display) -> String {
        let v_type = oracle.find(&self.inner);

        let getter = format!(
            "{vars}.get{vt}List({prop})",
            vars = vars,
            vt = v_type.variables_type(oracle),
            prop = identifiers::quoted(prop),
        );

        let mapper = match v_type.transform(oracle) {
            Some(item) => format!("?.mapNotNull({item})", item = item),
            _ => "".into(),
        };

        format!("{}{}", getter, mapper)
    }

    /// Accepts two runtime expressions and returns a runtime experession to combine. If the `default` is of type `T`,
    /// the `override` is of type `T?`.
    fn with_fallback(
        &self,
        _oracle: &dyn CodeOracle,
        overrides: &dyn Display,
        default: &dyn Display,
    ) -> String {
        format!(
            "{overrides} ?: {default}",
            overrides = overrides,
            default = default
        )
    }

    /// The name of the type as it's represented in the `Variables` object.
    /// The string return may be used to combine with an indentifier, e.g. a `Variables` method name.
    fn variables_type(&self, _oracle: &dyn CodeOracle) -> VariablesType {
        unimplemented!("Nesting lists in to lists and maps are not supported")
    }

    /// A representation of the given literal for this type.
    /// N.B. `Literal` is aliased from `interface::Literal`, so may not be whole suited to this task.
    fn literal(&self, oracle: &dyn CodeOracle, literal: &Literal) -> String {
        let variant = match literal {
            serde_json::Value::Array(v) => v,
            _ => unreachable!(),
        };

        let v_type = oracle.find(&self.inner);
        let src: Vec<String> = variant.iter().map(|v| v_type.literal(oracle, v)).collect();

        format!("listOf({})", src.join(", "))
    }
}
