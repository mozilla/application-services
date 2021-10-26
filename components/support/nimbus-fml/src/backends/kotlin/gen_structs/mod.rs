/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
//  use std::collections::HashSet;
//  use std::fmt;

//  use anyhow::Result;
use askama::Template;
use heck::{CamelCase, MixedCase, ShoutySnakeCase};
//  use serde::{Deserialize, Serialize};

//  use crate::bindings::backend::CodeDeclaration;
//  use crate::interface::*;
//  use crate::MergeWith;

use std::fmt;

use crate::backends::{CodeOracle, CodeType, TypeIdentifier};

mod enum_;
mod filters;
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

//  #[derive(Template)]
//  #[template(syntax = "kt", escape = "none", path = "wrapper.kt")]
//  pub struct KotlinWrapper<'a> {
//      config: Config,
//      ci: &'a FeatureManifest,
//      oracle: KotlinCodeOracle,
//  }
//  impl<'a> KotlinWrapper<'a> {
//      pub fn new(config: Config, ci: &'a FeatureManifest) -> Self {
//          Self {
//              config,
//              ci,
//              oracle: Default::default(),
//          }
//      }

//      pub fn members(&self) -> Vec<Box<dyn CodeDeclaration + 'a>> {
//          let ci = self.ci;
//          vec![
//              Box::new(object::ObjectRuntime::new(ci)) as Box<dyn CodeDeclaration>,
//          ]
//          .into_iter()
//          .chain(
//              ci.iter_enum_definitions().into_iter().map(|inner| {
//                  Box::new(enum_::KotlinEnum::new(inner, ci)) as Box<dyn CodeDeclaration>
//              }),
//          )
//          .chain(ci.iter_function_definitions().into_iter().map(|inner| {
//              Box::new(function::KotlinFunction::new(inner, ci)) as Box<dyn CodeDeclaration>
//          }))
//          .chain(ci.iter_decorator_definitions().into_iter().map(|inner| {
//              Box::new(decorator::KotlinDecoratorObject::new(inner, ci)) as Box<dyn CodeDeclaration>
//          }))
//          .chain(ci.iter_object_definitions().into_iter().map(|inner| {
//              Box::new(object::KotlinObject::new(inner, ci)) as Box<dyn CodeDeclaration>
//          }))
//          .chain(ci.iter_record_definitions().into_iter().map(|inner| {
//              Box::new(record::KotlinRecord::new(inner, ci)) as Box<dyn CodeDeclaration>
//          }))
//          .chain(
//              ci.iter_error_definitions().into_iter().map(|inner| {
//                  Box::new(error::KotlinError::new(inner, ci)) as Box<dyn CodeDeclaration>
//              }),
//          )
//          .chain(
//              ci.iter_callback_interface_definitions()
//                  .into_iter()
//                  .map(|inner| {
//                      Box::new(callback_interface::KotlinCallbackInterface::new(inner, ci))
//                          as Box<dyn CodeDeclaration>
//                  }),
//          )
//          .collect()
//      }

//      pub fn initialization_code(&self) -> Vec<String> {
//          let oracle = &self.oracle;
//          self.members()
//              .into_iter()
//              .filter_map(|member| member.initialization_code(oracle))
//              .collect()
//      }

//      pub fn declaration_code(&self) -> Vec<String> {
//          let oracle = &self.oracle;
//          self.members()
//              .into_iter()
//              .filter_map(|member| member.definition_code(oracle))
//              .chain(
//                  self.ci
//                      .iter_types()
//                      .into_iter()
//                      .filter_map(|type_| oracle.find(&type_).helper_code(oracle)),
//              )
//              .collect()
//      }

//      pub fn imports(&self) -> Vec<String> {
//          let oracle = &self.oracle;
//          let mut imports: Vec<String> = self
//              .members()
//              .into_iter()
//              .filter_map(|member| member.imports(oracle))
//              .flatten()
//              .chain(
//                  self.ci
//                      .iter_types()
//                      .into_iter()
//                      .filter_map(|type_| oracle.find(&type_).imports(oracle))
//                      .flatten(),
//              )
//              .collect::<HashSet<String>>()
//              .into_iter()
//              .collect();

//          imports.sort();
//          imports
//      }
//  }

#[derive(Default)]
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

    /// Get the idiomatic Kotlin rendering of a class name (for enums, records, errors, etc).
    fn class_name(&self, nm: &dyn fmt::Display) -> String {
        nm.to_string().to_camel_case()
    }

    /// Get the idiomatic Kotlin rendering of a function name.
    fn fn_name(&self, nm: &dyn fmt::Display) -> String {
        nm.to_string().to_mixed_case()
    }

    /// Get the idiomatic Kotlin rendering of a variable name.
    fn var_name(&self, nm: &dyn fmt::Display) -> String {
        nm.to_string().to_mixed_case()
    }

    /// Get the idiomatic Kotlin rendering of an individual enum variant.
    fn enum_variant_name(&self, nm: &dyn fmt::Display) -> String {
        nm.to_string().to_shouty_snake_case()
    }

    /// Get the idiomatic Kotlin rendering of an exception name
    ///
    /// This replaces "Error" at the end of the name with "Exception".  Rust code typically uses
    /// "Error" for any type of error but in the Java world, "Error" means a non-recoverable error
    /// and is distinguished from an "Exception".
    fn error_name(&self, nm: &dyn fmt::Display) -> String {
        let name = nm.to_string();
        match name.strip_suffix("Error") {
            None => name,
            Some(stripped) => {
                let mut kt_exc_name = stripped.to_owned();
                kt_exc_name.push_str("Exception");
                kt_exc_name
            }
        }
    }
}
