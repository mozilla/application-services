use std::collections::HashMap;

/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The `TypeRef` enum defines a reference to a type.
///
/// Other types will be defined in terms of these enum values.
///
/// They represent the types available via the current `Variables` API—
/// some primitives and structural types— and can be represented by
/// Kotlin, Swift and JSON Schema.
///
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub enum TypeRef {
    // Current primitives.
    String,
    Int,
    Boolean,

    // Strings can be coerced into a few types.
    // The types here will require the app's bundle or context to look up the final value.
    // They will likely have
    BundleText(StringId),
    BundleImage(StringId),

    Enum(String),
    // JSON objects can represent a data class.
    Object(String),

    // JSON objects can also represent a `Map<String, V>` or a `Map` with
    // keys that can be derived from a string.
    StringMap(Box<TypeRef>),
    // We can coerce the String keys into Enums, so this repesents that.
    EnumMap(Box<TypeRef>, Box<TypeRef>),

    List(Box<TypeRef>),
    Option(Box<TypeRef>),
}

pub(crate) type StringId = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FeatureManifest {
    pub enum_defs: Vec<EnumDef>,
    pub obj_defs: Vec<ObjectDef>,
    // `hints` are useful for things that will be constructed from strings
    // such as images and display text.
    pub hints: HashMap<StringId, FromStringDef>,
    pub feature_defs: Vec<FeatureDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FeatureDef {
    name: String,
    doc: String,
    props: Vec<PropDef>,
    default: Option<Literal>,
}
impl FeatureDef {
    #[allow(dead_code)]
    pub fn new(name: &str, doc: &str, props: Vec<PropDef>, default: Option<Literal>) -> Self {
        Self {
            name: name.into(),
            doc: doc.into(),
            props,
            default,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EnumDef {
    pub name: String,
    pub doc: String,
    pub variants: Vec<VariantDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FromStringDef {
    pub name: String,
    pub doc: String,
    pub variants: Vec<VariantDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VariantDef {
    name: String,
    doc: String,
}
impl VariantDef {
    #[allow(dead_code)]
    pub fn new(name: &str, doc: &str) -> Self {
        Self {
            name: name.into(),
            doc: doc.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObjectDef {
    name: String,
    doc: String,
    props: Vec<PropDef>,
}
impl ObjectDef {
    #[allow(dead_code)]
    pub fn new(name: &str, doc: &str, props: Vec<PropDef>) -> Self {
        Self {
            name: name.into(),
            doc: doc.into(),
            props,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PropDef {
    pub name: String,
    pub doc: String,
    pub typ: TypeRef,
    pub default: Literal,
}

type Literal = Value;

#[cfg(test)]
mod unit_tests {
    use super::*;
    use crate::error::Result;
    use crate::fixtures::intermediate_representation;

    #[test]
    fn can_ir_represent_smoke_test() -> Result<()> {
        let m1 = intermediate_representation::get_simple_homescreen_feature();
        let string = serde_json::to_string(&m1)?;
        let m2: FeatureManifest = serde_json::from_str(&string)?;

        assert_eq!(m1, m2);

        Ok(())
    }
}
