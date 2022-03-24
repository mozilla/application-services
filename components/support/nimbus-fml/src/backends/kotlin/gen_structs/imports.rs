/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::fmt::Display;

use super::{filters, object::object_literal};
use crate::{
    backends::{CodeDeclaration, CodeOracle, LiteralRenderer, TypeIdentifier},
    intermediate_representation::{FeatureManifest, ImportedModule, Literal},
};
use askama::Template;

#[derive(Template)]
#[template(
    syntax = "kt",
    escape = "none",
    path = "ImportedModuleInitializationTemplate.kt"
)]
pub(crate) struct ImportedModuleInitialization<'a> {
    pub(crate) fm: FeatureManifest,
    pub(crate) inner: ImportedModule<'a>,
}

impl<'a> ImportedModuleInitialization<'a> {
    pub(crate) fn new(fm: &FeatureManifest, inner: &ImportedModule<'a>) -> Self {
        Self {
            fm: fm.clone(),
            inner: inner.clone(),
        }
    }
}

impl CodeDeclaration for ImportedModuleInitialization<'_> {
    fn imports(&self, _oracle: &dyn CodeOracle) -> Option<Vec<String>> {
        let p = self.inner.about.nimbus_package_name()?;
        Some(vec![format!("{}.*", p)])
    }

    fn initialization_code(&self, _oracle: &dyn CodeOracle) -> Option<String> {
        Some(self.render().unwrap())
    }

    fn definition_code(&self, _oracle: &dyn CodeOracle) -> Option<String> {
        None
    }
}

impl LiteralRenderer for ImportedModuleInitialization<'_> {
    fn literal(
        &self,
        oracle: &dyn CodeOracle,
        typ: &TypeIdentifier,
        value: &Literal,
        ctx: &dyn Display,
    ) -> String {
        object_literal(&self.fm, ctx, &self, oracle, typ, value)
    }
}
