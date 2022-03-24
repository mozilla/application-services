/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use askama::Template;
use std::collections::HashSet;

use crate::{
    backends::{CodeDeclaration, CodeOracle, CodeType, TypeIdentifier},
    intermediate_representation::{FeatureDef, FeatureManifest, TypeFinder},
    parser::AboutBlock,
};

mod bundled;
mod common;
mod enum_;
mod feature;
mod filters;
mod object;
mod primitives;
mod structural;

impl AboutBlock {
    fn nimbus_fully_qualified_name(&self) -> String {
        let specific = self.kotlin_about.as_ref().unwrap();

        let class = &specific.class;
        if class.starts_with('.') {
            format!("{}{}", specific.package, class)
        } else {
            class.clone()
        }
    }

    fn nimbus_object_name(&self) -> String {
        let specific = self.kotlin_about.as_ref().unwrap();
        let last = specific.class.split('.').last().unwrap_or(&specific.class);
        last.to_string()
    }

    fn nimbus_package_name(&self) -> Option<String> {
        let fqe = self.nimbus_fully_qualified_name();
        if !fqe.contains('.') {
            return None;
        }
        let mut it = fqe.split('.');
        it.next_back()?;
        Some(it.collect::<Vec<&str>>().join("."))
    }

    fn resource_package_name(&self) -> String {
        let specific = self.kotlin_about.as_ref().unwrap();
        specific.package.clone()
    }
}
#[derive(Template)]
#[template(syntax = "kt", escape = "none", path = "FeatureManifestTemplate.kt")]
pub struct FeatureManifestDeclaration<'a> {
    fm: &'a FeatureManifest,
    oracle: ConcreteCodeOracle,
}
impl<'a> FeatureManifestDeclaration<'a> {
    pub fn new(fm: &'a FeatureManifest) -> Self {
        Self {
            fm,
            oracle: Default::default(),
        }
    }

    pub fn members(&self) -> Vec<Box<dyn CodeDeclaration + 'a>> {
        let fm = self.fm;

        fm.iter_feature_defs()
            .into_iter()
            .map(|inner| {
                Box::new(feature::FeatureCodeDeclaration::new(fm, inner))
                    as Box<dyn CodeDeclaration>
            })
            .chain(fm.iter_enum_defs().map(|inner| {
                Box::new(enum_::EnumCodeDeclaration::new(fm, inner)) as Box<dyn CodeDeclaration>
            }))
            .chain(fm.iter_object_defs().into_iter().map(|inner| {
                Box::new(object::ObjectCodeDeclaration::new(fm, inner)) as Box<dyn CodeDeclaration>
            }))
            .collect()
    }

    pub fn iter_feature_defs(&self) -> Vec<&FeatureDef> {
        self.fm.iter_feature_defs().collect::<_>()
    }

    pub fn initialization_code(&self) -> Vec<String> {
        let oracle = &self.oracle;
        self.members()
            .into_iter()
            .filter_map(|member| member.initialization_code(oracle))
            .collect()
    }

    pub fn declaration_code(&self) -> Vec<String> {
        let oracle = &self.oracle;
        self.members()
            .into_iter()
            .filter_map(|member| member.definition_code(oracle))
            .collect()
    }

    pub fn imports(&self) -> Vec<String> {
        let oracle = &self.oracle;
        let mut imports: Vec<String> = self
            .members()
            .into_iter()
            .filter_map(|member| member.imports(oracle))
            .flatten()
            .chain(
                self.fm
                    .all_types()
                    .into_iter()
                    .filter_map(|type_| self.oracle.find(&type_).imports(oracle))
                    .flatten(),
            )
            .chain(vec![
                "org.mozilla.experiments.nimbus.Variables".to_string(),
                "org.mozilla.experiments.nimbus.FeaturesInterface".to_string(),
                format!("{}.R", self.fm.about.resource_package_name()),
            ])
            .collect::<HashSet<String>>()
            .into_iter()
            .collect();

        imports.sort();
        imports
    }
}

#[derive(Default, Clone)]
pub struct ConcreteCodeOracle;

impl ConcreteCodeOracle {
    fn create_code_type(&self, type_: TypeIdentifier) -> Box<dyn CodeType> {
        match type_ {
            TypeIdentifier::Boolean => Box::new(primitives::BooleanCodeType),
            TypeIdentifier::String => Box::new(primitives::StringCodeType),
            TypeIdentifier::Int => Box::new(primitives::IntCodeType),

            TypeIdentifier::BundleText(_) => Box::new(bundled::TextCodeType),
            TypeIdentifier::BundleImage(_) => Box::new(bundled::ImageCodeType),

            TypeIdentifier::Enum(id) => Box::new(enum_::EnumCodeType::new(id)),
            TypeIdentifier::Object(id) => Box::new(object::ObjectCodeType::new(id)),

            TypeIdentifier::Option(ref inner) => Box::new(structural::OptionalCodeType::new(inner)),
            TypeIdentifier::List(ref inner) => Box::new(structural::ListCodeType::new(inner)),
            TypeIdentifier::StringMap(ref v_type) => {
                let k_type = &TypeIdentifier::String;
                Box::new(structural::MapCodeType::new(k_type, v_type))
            }
            TypeIdentifier::EnumMap(ref k_type, ref v_type) => {
                Box::new(structural::MapCodeType::new(k_type, v_type))
            }
        }
    }
}

impl CodeOracle for ConcreteCodeOracle {
    fn find(&self, type_: &TypeIdentifier) -> Box<dyn CodeType> {
        self.create_code_type(type_.clone())
    }
}
