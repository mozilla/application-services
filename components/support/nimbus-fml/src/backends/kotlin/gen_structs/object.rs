/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::fmt::Display;

use crate::backends::{CodeDeclaration, CodeOracle, CodeType};
use crate::intermediate_representation::{self, FeatureManifest, ObjectDef};
use askama::Template;

use super::{identifiers, ConcreteCodeOracle};

pub struct ObjectRuntime;

impl CodeDeclaration for ObjectRuntime {}

pub struct ObjectCodeType {
    id: String,
}

impl ObjectCodeType {
    pub fn new(id: String) -> Self {
        Self { id }
    }
}

impl CodeType for ObjectCodeType {
    fn type_label(&self, _oracle: &dyn CodeOracle) -> String {
        identifiers::class_name(&self.id)
    }

    fn literal(
        &self,
        oracle: &dyn CodeOracle,
        _literal: &intermediate_representation::Literal,
    ) -> String {
        unimplemented!("Unimplemented for {}", self.type_label(oracle))
    }

    fn get_value(
        &self,
        _oracle: &dyn CodeOracle,
        _vars: &dyn std::fmt::Display,
        _prop: &dyn std::fmt::Display,
    ) -> String {
        todo!()
    }

    /// Accepts two runtime expressions and returns a runtime experession to combine. If the `default` is of type `T`,
    /// the `override` is of type `T?`.
    fn with_fallback(
        &self,
        _oracle: &dyn CodeOracle,
        _overrides: &dyn Display,
        _default: &dyn Display,
    ) -> String {
        todo!()
    }
}

#[derive(Template)]
#[template(syntax = "kt", escape = "none", path = "ObjectTemplate.kt")]
pub(crate) struct ObjectCodeDeclaration {
    _inner: ObjectDef,
    _oracle: ConcreteCodeOracle,
}

impl ObjectCodeDeclaration {
    pub fn new(_fm: &FeatureManifest, inner: &ObjectDef) -> Self {
        Self {
            _oracle: Default::default(),
            _inner: inner.clone(),
        }
    }
}

impl CodeDeclaration for ObjectCodeDeclaration {}
