/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use askama::Template;
use std::fmt::Display;

use super::filters;
use crate::{
    backends::{CodeOracle, CodeType, TypeIdentifier},
    intermediate_representation::{EnumDef, FeatureManifest, Literal},
};

pub(crate) struct EnumCodeType {
    id: String,
}

impl EnumCodeType {
    pub(crate) fn new(id: String) -> Self {
        Self { id }
    }
}

impl CodeType for EnumCodeType {
    /// The language specific label used to reference this type. This will be used in
    /// method signatures and property declarations.
    fn type_label(&self, oracle: &dyn CodeOracle) -> String {
        oracle.class_name(&self.id)
    }

    /// The language specific expression that gets a value of the `prop` from the `vars` object.
    fn get_value(&self, oracle: &dyn CodeOracle, vars: &dyn Display, prop: &dyn Display) -> String {
        format!(
            "{}.getEnum<{}>({})",
            vars,
            self.type_label(oracle),
            filters::quoted(prop)
        )
    }

    /// A representation of the given literal for this type.
    /// N.B. `Literal` is aliased from `interface::Literal`, so may not be whole suited to this task.
    fn literal(&self, oracle: &dyn CodeOracle, literal: &Literal) -> String {
        let variant = match literal {
            serde_json::Value::String(v) => v,
            _ => unreachable!(),
        };

        format!(
            "{}.{}",
            self.type_label(oracle),
            oracle.enum_variant_name(variant)
        )
    }
}

pub(crate) struct EnumMapCodeType {
    k_type: TypeIdentifier,
    v_type: TypeIdentifier,
}

impl EnumMapCodeType {
    pub(crate) fn new(k: &TypeIdentifier, v: &TypeIdentifier) -> Self {
        Self {
            k_type: k.clone(),
            v_type: v.clone(),
        }
    }
}

impl CodeType for EnumMapCodeType {
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
        // This cannot work: getStringMap gets a map of strings.
        // We might end up doing some horrible thing to match value types with canonical name.
        format!(
            "{vars}.getStringMap({prop}).mapKeysAsEnums<{k}, {v}>()",
            vars = vars,
            k = oracle.find(&self.k_type).type_label(oracle),
            v = oracle.find(&self.v_type).type_label(oracle),
            prop = filters::quoted(prop)
        )
    }

    /// A representation of the given literal for this type.
    /// N.B. `Literal` is aliased from `interface::Literal`, so may not be whole suited to this task.
    fn literal(&self, oracle: &dyn CodeOracle, literal: &Literal) -> String {
        let variant = match literal {
            serde_json::Value::Object(v) => v,
            _ => unreachable!(),
        };

        let k_type = oracle.find(&self.k_type);
        let v_type = oracle.find(&self.k_type);
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

        format!("mapOf({})", src.join(","))
    }
}

pub(crate) struct EnumCodeDeclaration<'oracle> {
    inner: EnumDef,
    oracle: &'oracle dyn CodeOracle,
}

impl<'oracle> EnumCodeDeclaration<'oracle> {
    pub fn new(oracle: &'oracle dyn CodeOracle, _fm: &FeatureManifest, inner: EnumDef) -> Self {
        Self { oracle, inner }
    }
}
