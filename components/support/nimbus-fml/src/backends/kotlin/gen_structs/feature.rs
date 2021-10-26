/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use askama::Template;

use super::filters;
use super::ConcreteCodeOracle;
use crate::{
    backends::{CodeDeclaration, CodeOracle},
    intermediate_representation::{FeatureDef, FeatureManifest},
};

#[derive(Template)]
#[template(syntax = "kt", escape = "none", path = "FeatureTemplate.kt")]
pub(crate) struct FeatureCodeDeclaration {
    inner: FeatureDef,
    oracle: ConcreteCodeOracle,
}

impl FeatureCodeDeclaration {
    pub fn new(_fm: &FeatureManifest, inner: &FeatureDef) -> Self {
        Self {
            oracle: Default::default(),
            inner: inner.clone(),
        }
    }
    pub fn inner(&self) -> &FeatureDef {
        &self.inner
    }
}

impl CodeDeclaration for FeatureCodeDeclaration {
    fn definition_code(&self, _oracle: &dyn CodeOracle) -> Option<String> {
        Some(self.render().unwrap())
    }
}
