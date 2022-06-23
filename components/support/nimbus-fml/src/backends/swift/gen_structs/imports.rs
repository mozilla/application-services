/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    backends::{CodeDeclaration, CodeOracle},
    intermediate_representation::ImportedModule,
};

pub(crate) struct ImportedModuleInitialization<'a> {
    pub(crate) inner: ImportedModule<'a>,
}

impl<'a> ImportedModuleInitialization<'a> {
    pub(crate) fn new(inner: &ImportedModule<'a>) -> Self {
        Self {
            inner: inner.clone(),
        }
    }
}

impl CodeDeclaration for ImportedModuleInitialization<'_> {
    fn imports(&self, _oracle: &dyn CodeOracle) -> Option<Vec<String>> {
        let p = self.inner.about().nimbus_module_name();
        Some(vec![p])
    }

    fn initialization_code(&self, _oracle: &dyn CodeOracle) -> Option<String> {
        None
    }

    fn definition_code(&self, _oracle: &dyn CodeOracle) -> Option<String> {
        None
    }
}
