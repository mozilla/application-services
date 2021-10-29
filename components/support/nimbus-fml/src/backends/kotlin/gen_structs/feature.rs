/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use askama::Template;

use super::filters;
use crate::{
    backends::{CodeDeclaration, CodeOracle},
    intermediate_representation::{FeatureDef, FeatureManifest},
    Config,
};

#[derive(Template)]
#[template(syntax = "kt", escape = "none", path = "FeatureTemplate.kt")]
pub(crate) struct FeatureCodeDeclaration {
    nimbus_object_name: String,
    inner: FeatureDef,
}

impl FeatureCodeDeclaration {
    pub fn new(_fm: &FeatureManifest, config: &Config, inner: &FeatureDef) -> Self {
        Self {
            nimbus_object_name: config.nimbus_object_name(),
            inner: inner.clone(),
        }
    }
    pub fn inner(&self) -> &FeatureDef {
        &self.inner
    }
    pub fn nimbus_object_name(&self) -> &String {
        &self.nimbus_object_name
    }
}

impl CodeDeclaration for FeatureCodeDeclaration {
    fn definition_code(&self, _oracle: &dyn CodeOracle) -> Option<String> {
        Some(self.render().unwrap())
    }
}
