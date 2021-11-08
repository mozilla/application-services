/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! # Backend traits
//!
//! This module provides a number of traits useful for implementing a backend for FML structs.
//!
//! A [CodeType] is needed for each type that is referred to in the feature definition (i.e. every [TypeRef]
//! instance should have corresponding `CodeType` instance). Helper code for types might include managing how merging/overriding of
//! defaults occur.
//!
//! A [CodeDeclaration] is needed for each type that is declared in the manifest file: i.e. an Object classes, Enum classes and Feature classes.
//! This has access to intermediate structs of the [crate::intermediate_representation::FeatureManifest] so may want to do some additional lookups to help rendering.
//!
//! `CodeDeclaration`s provide the target language's version of the type defined in the feature manifest. For objects and features, this would
//! be objects that have properties corresponding to the FML variables. For enums, this would mean the Enum class definition. In all cases, this will
//! likely be attached to an [askama::Template].
//!
//! `CodeDeclaration`s can also be used to conditionally include code: e.g. only include the CallbackInterfaceRuntime
//! if the user has used at least one callback interface.
//!
//! Each backend has a wrapper template for each file it needs to generate. This should collect the `CodeDeclaration`s that
//! the backend and `FeatureManifest` between them specify and use them to stitch together a file in the target language.
//!
//! The [CodeOracle] provides methods to map the `TypeRef` values found in the `FeatureManifest` to the `CodeType`s specified
//! by the backend.
//!
//! Each backend will have its own `filter` module, which is used by the askama templates used in all `CodeType`s and `CodeDeclaration`s.
//! This filter provides methods to generate expressions and identifiers in the target language. These are all forwarded to the oracle.

use std::fmt::Display;

use crate::intermediate_representation::Literal;
use crate::intermediate_representation::TypeRef;

pub type TypeIdentifier = TypeRef;

/// An object to look up a foreign language code specific renderer for a given type used.
/// Every [TypeRef] referred to in the [crate::intermediate_representation::FeatureManifest] should map to a corresponding
/// `CodeType`.
///
/// The mapping may be opaque, but the oracle always knows the answer.
pub trait CodeOracle {
    fn find(&self, type_: &TypeIdentifier) -> Box<dyn CodeType>;
}

/// A Trait to emit foreign language code to handle referenced types.
/// A type which is specified in the FML (i.e. a type that a variable declares itself of)
/// will have a `CodeDeclaration` as well, but for types used e.g. primitive types, Strings, etc
/// only a `CodeType` is needed.
pub trait CodeType {
    /// The language specific label used to reference this type. This will be used in
    /// method signatures and property declarations.
    fn type_label(&self, oracle: &dyn CodeOracle) -> String;

    /// The language specific expression that gets a value of the `prop` from the `vars` object.
    fn get_value(&self, oracle: &dyn CodeOracle, vars: &dyn Display, prop: &dyn Display) -> String {
        format!(
            "{vars}.get{vt}(\"{prop}\")",
            vars = vars,
            vt = self.variables_type(oracle),
            prop = prop
        )
    }

    /// The name of the type as it's represented in the `Variables` object.
    /// The string return may be used to combine with an indentifier, e.g. a `Variables` method name.
    fn variables_type(&self, _oracle: &dyn CodeOracle) -> VariablesType;

    /// A function handle that is capable of turning the variables type to the TypeRef type.
    fn transform(&self, _oracle: &dyn CodeOracle) -> Option<String> {
        None
    }

    /// Accepts two runtime expressions and returns a runtime experession to combine. If the `default` is of type `T`,
    /// the `override` is of type `T?`.
    fn with_fallback(
        &self,
        oracle: &dyn CodeOracle,
        overrides: &dyn Display,
        default: &dyn Display,
    ) -> String;

    /// A representation of the given literal for this type.
    /// N.B. `Literal` is aliased from `serde_json::Value`.
    fn literal(
        &self,
        oracle: &dyn CodeOracle,
        _renderer: &dyn LiteralRenderer,
        _literal: &Literal,
    ) -> String {
        unimplemented!("Unimplemented for {}", self.type_label(oracle))
    }

    /// Optional helper code to make this type work.
    /// This might include functions to patch a default value with another.
    fn helper_code(&self, _oracle: &dyn CodeOracle) -> Option<String> {
        None
    }

    /// A list of imports that are needed if this type is in use.
    /// Classes are imported exactly once.
    fn imports(&self, _oracle: &dyn CodeOracle) -> Option<Vec<String>> {
        None
    }
}

pub trait LiteralRenderer {
    fn literal(&self, _oracle: &dyn CodeOracle, _typ: &TypeIdentifier, value: &Literal) -> String;
}

impl<T, C> LiteralRenderer for T
where
    T: std::ops::Deref<Target = C>,
    C: LiteralRenderer,
{
    fn literal(&self, oracle: &dyn CodeOracle, typ: &TypeIdentifier, value: &Literal) -> String {
        self.deref().literal(oracle, typ, value)
    }
}

/// A trait that is able to render a declaration about a particular member declared in
/// the `FeatureManifest`.
/// Like `CodeType`, it can render declaration code and imports.
/// All methods are optional, and there is no requirement that the trait be used for a particular
/// member. Thus, it can also be useful for conditionally rendering code.
pub trait CodeDeclaration {
    /// A list of imports that are needed if this type is in use.
    /// Classes are imported exactly once.
    fn imports(&self, _oracle: &dyn CodeOracle) -> Option<Vec<String>> {
        None
    }

    /// Code (one or more statements) that is run on start-up of the library,
    /// but before the client code has access to it.
    fn initialization_code(&self, _oracle: &dyn CodeOracle) -> Option<String> {
        None
    }

    /// Code which represents this member. e.g. the foreign language class definition for
    /// a given Object type.
    fn definition_code(&self, _oracle: &dyn CodeOracle) -> Option<String> {
        None
    }
}

/// The generated code is running against hand written code to give type safe, error free access to JSON.
/// This is the `Variables` object. This enum gives the underlying types that the `Variables` object supports.
pub enum VariablesType {
    Bool,
    #[allow(dead_code)]
    Image,
    Int,
    String,
    #[allow(dead_code)]
    Text,
    Variables,
}

/// The Variables objects use a naming convention to name its methods. e.g. `getBool`, `getBoolList`, `getBoolMap`.
/// In part this is to make generating code easier.
/// This is the mapping from type to identifier part that corresponds to its type.
impl Display for VariablesType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let nm = match self {
            VariablesType::Bool => "Bool",
            VariablesType::Image => "Image",
            VariablesType::Int => "Int",
            VariablesType::String => "String",
            VariablesType::Text => "Text",
            VariablesType::Variables => "Variables",
        };
        f.write_str(nm)
    }
}

pub(crate) mod kotlin;
