/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use askama::Template;
use std::fmt::Display;

use super::filters;
use super::identifiers;
use crate::backends::VariablesType;
use crate::{
    backends::{CodeDeclaration, CodeOracle, CodeType, TypeIdentifier},
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
    fn type_label(&self, _oracle: &dyn CodeOracle) -> String {
        identifiers::class_name(&self.id)
    }

    /// The language specific expression that gets a value of the `prop` from the `vars` object.
    fn get_value(&self, oracle: &dyn CodeOracle, vars: &dyn Display, prop: &dyn Display) -> String {
        format!(
            "{vars}.getString({prop}, {transform})",
            vars = vars,
            transform = self.transform(oracle).unwrap(),
            prop = identifiers::quoted(prop)
        )
    }

    /// The name of the type as it's represented in the `Variables` object.
    /// The string return may be used to combine with an indentifier, e.g. a `Variables` method name.
    fn variables_type(&self, _oracle: &dyn CodeOracle) -> VariablesType {
        VariablesType::String
    }

    /// A function handle that is capable of turning the variables type to the TypeRef type.
    fn transform(&self, oracle: &dyn CodeOracle) -> Option<String> {
        Some(format!(
            "{enum_type}::enumValue",
            enum_type = self.type_label(oracle)
        ))
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
            identifiers::enum_variant_name(variant)
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

        format!("mapOf({})", src.join(","))
    }
}

#[derive(Template)]
#[template(syntax = "kt", escape = "none", path = "EnumTemplate.kt")]
pub(crate) struct EnumCodeDeclaration {
    inner: EnumDef,
}

impl EnumCodeDeclaration {
    pub fn new(_fm: &FeatureManifest, inner: &EnumDef) -> Self {
        Self {
            inner: inner.clone(),
        }
    }
    fn inner(&self) -> EnumDef {
        self.inner.clone()
    }
}

impl CodeDeclaration for EnumCodeDeclaration {
    fn definition_code(&self, _oracle: &dyn CodeOracle) -> Option<String> {
        Some(self.render().unwrap())
    }
}
