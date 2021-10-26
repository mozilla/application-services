/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use askama::Template;
use std::collections::HashSet;

//  use anyhow::Result;
//  use serde::{Deserialize, Serialize};

//  use crate::bindings::backend::CodeDeclaration;
//  use crate::interface::*;
//  use crate::MergeWith;

use crate::{
    backends::{CodeDeclaration, CodeOracle, CodeType, TypeIdentifier},
    intermediate_representation::FeatureManifest,
    Config,
};

mod enum_;
mod feature;
mod filters;
mod identifiers;
mod object;
mod primitives;
mod structural;

//  // Some config options for it the caller wants to customize the generated Kotlin.
//  // Note that this can only be used to control details of the Kotlin *that do not affect the underlying component*,
//  // sine the details of the underlying component are entirely determined by the `FeatureManifest`.
//  #[derive(Debug, Default, Clone, Serialize, Deserialize)]
//  pub struct Config {
//      package_name: Option<String>,
//      cdylib_name: Option<String>,
//  }

//  impl Config {
//      pub fn package_name(&self) -> String {
//          if let Some(package_name) = &self.package_name {
//              package_name.clone()
//          } else {
//              "uniffi".into()
//          }
//      }

//      pub fn cdylib_name(&self) -> String {
//          if let Some(cdylib_name) = &self.cdylib_name {
//              cdylib_name.clone()
//          } else {
//              "uniffi".into()
//          }
//      }
//  }

// //  impl From<&FeatureManifest> for Config {
// //      fn from(ci: &FeatureManifest) -> Self {
// //          Config {
// //              package_name: Some(format!("uniffi.{}", ci.namespace())),
// //              cdylib_name: Some(format!("uniffi_{}", ci.namespace())),
// //          }
// //      }
// //  }

// //  impl MergeWith for Config {
// //      fn merge_with(&self, other: &Self) -> Self {
// //          Config {
// //              package_name: self.package_name.merge_with(&other.package_name),
// //              cdylib_name: self.cdylib_name.merge_with(&other.cdylib_name),
// //          }
// //      }
// //  }

#[derive(Template)]
#[template(syntax = "kt", escape = "none", path = "FeatureManifestTemplate.kt")]
pub struct FeatureManifestDeclaration<'a> {
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
        let fm = self.fm;
        vec![
           //  Box::new(object::ObjectRuntime::new(ci)) as Box<dyn CodeDeclaration>,
        ]
        .into_iter()
        .chain(fm.iter_feature_defs().into_iter().map(|inner| {
            Box::new(feature::FeatureCodeDeclaration::new(fm, inner)) as Box<dyn CodeDeclaration>
        }))
        .chain(fm.iter_enum_defs().map(|inner| {
            Box::new(enum_::EnumCodeDeclaration::new(fm, inner)) as Box<dyn CodeDeclaration>
        }))
        //  .chain(fm.iter_object_defs().into_iter().map(|inner| {
        //      Box::new(object::ObjectDef::new(inner, fm)) as Box<dyn CodeDeclaration>
        //  }))
        .collect()
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
            //  .chain(
            //      self.fm
            //          .iter_types()
            //          .into_iter()
            //          .filter_map(|type_| oracle.find(&type_).helper_code(oracle)),
            //  )
            .collect()
    }

    pub fn imports(&self) -> Vec<String> {
        let oracle = &self.oracle;
        let mut imports: Vec<String> = self
            .members()
            .into_iter()
            .filter_map(|member| member.imports(oracle))
            .flatten()
            //  .chain(
            //      self.ci
            //          .iter_types()
            //          .into_iter()
            //          .filter_map(|type_| oracle.find(&type_).imports(oracle))
            //          .flatten(),
            //  )
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
        // I really want access to the FeatureManifest here so I can look up the interface::{Enum, Record, Error, Object, etc}
        // However, there's some violence and gore I need to do to (temporarily) make the oracle usable from filters.

        // Some refactor of the templates is needed to make progress here: I think most of the filter functions need to take an &dyn CodeOracle
        match type_ {
            TypeIdentifier::Boolean => Box::new(primitives::BooleanCodeType),
            TypeIdentifier::String => Box::new(primitives::StringCodeType),
            TypeIdentifier::Int => Box::new(primitives::IntCodeType),

            TypeIdentifier::Enum(id) => Box::new(enum_::EnumCodeType::new(id)),
            // TypeIdentifier::Object(id) => Box::new(object::ObjectCodeType::new(id)),

            // TypeIdentifier::Optional(ref inner) => {
            //     let outer = type_.clone();
            //     let inner = *inner.to_owned();
            //     Box::new(structural::OptionalCodeType::new(inner, outer))
            // }
            // TypeIdentifier::List(ref inner) => {
            //     let outer = type_.clone();
            //     let inner = *inner.to_owned();
            //     Box::new(structural::ListCodeType::new(inner, outer))
            // }
            // TypeIdentifier::StringMap(ref inner) => {
            //     let outer = type_.clone();
            //     let inner = *inner.to_owned();
            //     Box::new(structural::StringMapCodeType::new(inner, outer))
            // }
            // TypeIdentifier::EnumMap(ref k_type, ref v_type) => {
            //     Box::new(enum_::EnumMapCodeType::new(k_type, v_type))
            // }
            _ => unimplemented!(),
        }
    }
}

impl CodeOracle for ConcreteCodeOracle {
    fn find(&self, type_: &TypeIdentifier) -> Box<dyn CodeType> {
        self.create_code_type(type_.clone())
    }
}
