/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use askama::Template;
use std::collections::HashSet;

use crate::{
    backends::{CodeDeclaration, CodeOracle, CodeType, TypeIdentifier},
    intermediate_representation::{FeatureDef, FeatureManifest},
    Config,
};

//TODO:


#[derive(Template)]
#[template(syntax = "swift", escape = "none", path = "FeatureManifestTemplate.swift")]
pub struct FeatureManifestDeclaration<'a> {
    #[allow(dead_code)]
    config: Config,
    fm: &'a FeatureManifest,
    oracle: ConcreteCodeOracle,
}

impl<'a> FeatureManifestDeclaration<'a> {
    pub fn new(config: Config, fm: &'a FeatureManifest) -> Self {
        Self {
            config,
            fm,
            oracle: Default::default(),
        }
    }

    pub fn members(&self) -> Vec<Box<dyn CodeDeclaration + 'a>> {
        todo!()
    }

    pub fn iter_feature_defs(&self) -> Vec<&FeatureDef> {
        todo!()
    }

    pub fn initialization_code(&self) -> Vec<String> {
       todo!()
    }

    pub fn declaration_code(&self) -> Vec<String> {
        todo!()
    }

    pub fn imports(&self) -> Vec<String> {
        todo!()
    }
}

#[derive(Default, Clone)]
pub struct ConcreteCodeOracle;

impl ConcreteCodeOracle {
    fn create_code_type(&self, type_: TypeIdentifier) -> Box<dyn CodeType> {
       todo!()
    }
}

impl CodeOracle for ConcreteCodeOracle {
    fn find(&self, type_: &TypeIdentifier) -> Box<dyn CodeType> {
        self.create_code_type(type_.clone())
    }
}