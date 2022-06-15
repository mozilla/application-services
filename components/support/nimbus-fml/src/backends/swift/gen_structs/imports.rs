/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    backends::{CodeDeclaration, CodeOracle},
    intermediate_representation::ImportedClass,
};

pub(crate) struct ImportedClassInitialization<'a> {
    pub(crate) inner: ImportedClass<'a>,
}

impl<'a> ImportedClassInitialization<'a> {
    pub(crate) fn new(inner: &ImportedClass<'a>) -> Self {
        Self {
            inner: inner.clone(),
        }
    }
}

impl CodeDeclaration for ImportedClassInitialization<'_> {
    fn imports(&self, _oracle: &dyn CodeOracle) -> Option<Vec<String>> {
        let p = self.inner.about.nimbus_module_name();
        Some(vec![p])
    }

    fn initialization_code(&self, _oracle: &dyn CodeOracle) -> Option<String> {
        None
    }

    fn definition_code(&self, _oracle: &dyn CodeOracle) -> Option<String> {
        None
    }
}
