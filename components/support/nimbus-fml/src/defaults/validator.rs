/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::{did_you_mean, FMLError};
use crate::intermediate_representation::{FeatureDef, TypeRef};
use crate::{
    error::Result,
    intermediate_representation::{EnumDef, ObjectDef},
};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

pub(crate) struct DefaultsValidator<'a> {
    enum_defs: &'a BTreeMap<String, EnumDef>,
    object_defs: &'a BTreeMap<String, ObjectDef>,
}

impl<'a> DefaultsValidator<'a> {
    pub(crate) fn new(
        enum_defs: &'a BTreeMap<String, EnumDef>,
        object_defs: &'a BTreeMap<String, ObjectDef>,
    ) -> Self {
        Self {
            enum_defs,
            object_defs,
        }
    }

    pub(crate) fn validate_object_def(&self, object: &ObjectDef) -> Result<(), FMLError> {
        for prop in &object.props {
            let path = format!("objects/{}.{}", object.name, prop.name);
            let error_path = vec![prop.name.to_string()];
            self.validate_types(path.as_str(), &error_path, &prop.typ, &prop.default)?;
        }
        Ok(())
    }

    pub(crate) fn validate_feature_def(&self, feature_def: &FeatureDef) -> Result<()> {
        for prop in &feature_def.props {
            let path = format!("features/{}.{}", feature_def.name, prop.name);
            let error_path = vec![prop.name.to_string()];
            self.validate_types(path.as_str(), &error_path, &prop.typ, &prop.default)?;
        }
        Ok(())
    }

    fn get_enum(&self, nm: &str) -> Option<&EnumDef> {
        self.enum_defs.get(nm)
    }

    fn get_object(&self, nm: &str) -> Option<&ObjectDef> {
        self.object_defs.get(nm)
    }

    pub fn validate_types(
        &self,
        path: &str,
        error_path: &Vec<String>,
        type_ref: &TypeRef,
        default: &Value,
    ) -> Result<()> {
        match (type_ref, default) {
            (TypeRef::Boolean, Value::Bool(_))
            | (TypeRef::BundleImage, Value::String(_))
            | (TypeRef::BundleText, Value::String(_))
            | (TypeRef::String, Value::String(_))
            | (TypeRef::StringAlias(_), Value::String(_))
            | (TypeRef::Int, Value::Number(_))
            | (TypeRef::Option(_), Value::Null) => Ok(()),
            (TypeRef::Option(inner), v) => {
                if let TypeRef::Option(_) = inner.as_ref() {
                    return Err(FMLError::ValidationError(
                        path.to_string(),
                        "Nested options".into(),
                    ));
                }
                self.validate_types(path, error_path, inner, v)
            }
            (TypeRef::Enum(enum_name), Value::String(s)) => {
                let enum_def = self
                    .get_enum(enum_name)
                    // If this is thrown, there's a problem in validate_type_ref.
                    .unwrap_or_else(|| {
                        unreachable!("Enum {enum_name} is not defined in the manifest")
                    });
                let mut valid = HashSet::new();
                for variant in enum_def.variants() {
                    let name = variant.name();
                    if *s == name {
                        return Ok(());
                    }
                    valid.insert(name);
                }
                Err(FMLError::FeatureValidationError {
                    path: path.to_string(),
                    message: format!("\"{s}\" is not a valid {enum_name}{}", did_you_mean(valid)),
                    literals: append_quoted(error_path, s),
                })
            }
            (TypeRef::EnumMap(enum_type, map_type), Value::Object(map))
                if matches!(**enum_type, TypeRef::Enum(_)) =>
            {
                let enum_name = enum_type.name().unwrap();
                let enum_def = self
                    .get_enum(enum_name)
                    // If this is thrown, there's a problem in validate_type_ref.
                    .unwrap_or_else(|| {
                        unreachable!("Enum {enum_name} is not defined in the manifest")
                    });

                // We first validate that the keys of the map cover all all the enum variants, and no more or less
                let mut seen = HashSet::new();
                let mut unseen = HashSet::new();
                let mut valid = HashSet::new();
                for variant in enum_def.variants() {
                    let nm = variant.name();
                    valid.insert(nm.clone());

                    let map_value = map.get(&nm);
                    match (map_type.as_ref(), map_value) {
                        (TypeRef::Option(_), None) => (),
                        (_, None) => {
                            unseen.insert(variant.name());
                        }
                        (_, Some(inner)) => {
                            let path = format!("{path}[{}#{nm}]", enum_def.name);
                            let literals =
                                append(error_path, &["{".to_string(), format!("\"{nm}\"")]);
                            self.validate_types(&path, &literals, map_type, inner)?;
                            seen.insert(nm);
                        }
                    }
                }

                if !unseen.is_empty() {
                    return Err(FMLError::FeatureValidationError {
                        path: path.to_string(),
                        message: format!("Enum map {enum_name} is missing values for {unseen:?}"),
                        // Can we be more specific that just the opening brace?
                        literals: append1(error_path, "{"),
                    });
                }
                for map_key in map.keys() {
                    if !seen.contains(map_key) {
                        return Err(FMLError::FeatureValidationError {
                            path: path.to_string(),
                            message: format!("Invalid key \"{map_key}\"{}", did_you_mean(valid)),
                            literals: append(
                                error_path,
                                &["{".to_string(), format!("\"{map_key}\"")],
                            ),
                        });
                    }
                }
                Ok(())
            }
            (TypeRef::EnumMap(_, map_type), Value::Object(map)) // Map<string-alias, T>
            | (TypeRef::StringMap(map_type), Value::Object(map)) => {
                for (key, value) in map {
                    let path = format!("{path}['{key}']");
                    let literals = append(error_path, &["{".to_string(), format!("\"{key}\"")]);
                    self.validate_types(&path, &literals, map_type, value)?;
                }
                Ok(())
            }
            (TypeRef::List(list_type), Value::Array(arr)) => {
                let mut literals = append1(error_path, "[");
                for (index, value) in arr.iter().enumerate() {
                    let path = format!("{path}['{index}']");
                    self.validate_types(&path, &literals, list_type, value)?;
                    literals.push(",".to_string());
                }
                Ok(())
            }
            (TypeRef::Object(obj_name), Value::Object(map)) => {
                let obj_def = self
                    .get_object(obj_name)
                    // If this is thrown, there's a problem in validate_type_ref.
                    .unwrap_or_else(|| {
                        unreachable!("Object {obj_name} is not defined in the manifest")
                    });
                let mut valid = HashSet::new();
                let mut unseen = HashSet::new();
                let path = format!("{path}#{obj_name}");
                for prop in &obj_def.props {
                    // We only check the defaults overriding the property defaults
                    // from the object's own property defaults.
                    // We check the object property defaults previously.
                    let nm = prop.name();
                    if let Some(map_val) = map.get(&nm) {
                        let path = format!("{path}.{}", prop.name);
                        let literals = append(
                            error_path,
                            &["{".to_string(), format!("\"{}\"", &prop.name)],
                        );
                        self.validate_types(&path, &literals, &prop.typ, map_val)?;
                    } else {
                        unseen.insert(nm.clone());
                    }

                    valid.insert(nm);
                }
                for map_key in map.keys() {
                    if !valid.contains(map_key) {
                        return Err(FMLError::FeatureValidationError {
                            path,
                            message: format!(
                                "Invalid key \"{map_key}\" for object {obj_name}{}",
                                did_you_mean(valid)
                            ),
                            literals: append_quoted(error_path, map_key),
                        });
                    }
                }

                Ok(())
            }
            _ => Err(FMLError::FeatureValidationError {
                path: path.to_string(),
                message: format!("Mismatch between type {type_ref} and default {default}"),
                literals: append1(error_path, &default.to_string()),
            }),
        }
    }
}

fn append(original: &[String], new: &[String]) -> Vec<String> {
    let mut clone = original.to_owned();
    clone.extend(new.iter().cloned());
    clone
}

fn append1(original: &[String], new: &str) -> Vec<String> {
    let mut clone = original.to_owned();
    clone.push(new.to_string());
    clone
}

fn append_quoted(original: &[String], new: &str) -> Vec<String> {
    append1(original, &format!("\"{new}\""))
}

#[cfg(test)]
mod test_types {

    use serde_json::json;

    use crate::intermediate_representation::{PropDef, VariantDef};

    use super::*;

    impl DefaultsValidator<'_> {
        fn validate_prop_defaults(&self, prop: &PropDef) -> Result<()> {
            let error_path = Default::default();
            self.validate_types(prop.name.as_str(), &error_path, &prop.typ, &prop.default)
        }
    }

    fn enums() -> BTreeMap<String, EnumDef> {
        let enum_ = EnumDef {
            name: "ButtonColor".into(),
            variants: vec![
                VariantDef {
                    name: "blue".into(),
                    ..Default::default()
                },
                VariantDef {
                    name: "green".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        BTreeMap::from([(enum_.name(), enum_)])
    }

    fn objects() -> BTreeMap<String, ObjectDef> {
        let obj1 = ObjectDef {
            name: "SampleObj".into(),
            props: vec![
                PropDef::new("int", TypeRef::Int, json!(1)),
                PropDef::new("string", TypeRef::String, json!("a string")),
                PropDef::new("enum", TypeRef::Enum("ButtonColor".into()), json!("blue")),
                PropDef::new(
                    "list",
                    TypeRef::List(Box::new(TypeRef::Boolean)),
                    json!([true, false]),
                ),
                PropDef::new(
                    "optional",
                    TypeRef::Option(Box::new(TypeRef::Int)),
                    json!(null),
                ),
                PropDef::new(
                    "nestedObj",
                    TypeRef::Object("NestedObject".into()),
                    json!({
                        "enumMap": {
                            "blue": 1,
                        },
                    }),
                ),
            ],
            ..Default::default()
        };

        let obj2 = ObjectDef {
            name: "NestedObject".into(),
            props: vec![PropDef::new(
                "enumMap",
                TypeRef::EnumMap(
                    Box::new(TypeRef::Enum("ButtonColor".into())),
                    Box::new(TypeRef::Int),
                ),
                json!({
                    "blue": 4,
                    "green": 2,
                }),
            )],
            ..Default::default()
        };
        BTreeMap::from([(obj1.name(), obj1), (obj2.name(), obj2)])
    }

    #[test]
    fn test_validate_prop_defaults_string() -> Result<()> {
        let mut prop = PropDef::new("key", TypeRef::String, json!("default!"));
        let enums1 = Default::default();
        let objs = Default::default();
        let fm = DefaultsValidator::new(&enums1, &objs);
        fm.validate_prop_defaults(&prop)?;

        prop.default = json!(100);
        fm.validate_prop_defaults(&prop)
            .expect_err("Should error out, default is number when it should be string");
        Ok(())
    }

    #[test]
    fn test_validate_prop_defaults_int() -> Result<()> {
        let mut prop = PropDef::new("key", TypeRef::Int, json!(100));
        let enums1 = Default::default();
        let objs = Default::default();
        let fm = DefaultsValidator::new(&enums1, &objs);
        fm.validate_prop_defaults(&prop)?;
        prop.default = json!("100");

        fm.validate_prop_defaults(&prop)
            .expect_err("Should error out, default is string when it should be number");
        Ok(())
    }

    #[test]
    fn test_validate_prop_defaults_bool() -> Result<()> {
        let mut prop = PropDef::new("key", TypeRef::Boolean, json!(true));
        let enums1 = Default::default();
        let objs = Default::default();
        let fm = DefaultsValidator::new(&enums1, &objs);
        fm.validate_prop_defaults(&prop)?;
        prop.default = json!("100");

        fm.validate_prop_defaults(&prop)
            .expect_err("Should error out, default is string when it should be a boolean");
        Ok(())
    }

    #[test]
    fn test_validate_prop_defaults_bundle_image() -> Result<()> {
        let mut prop = PropDef::new("key", TypeRef::BundleImage, json!("IconBlue"));
        let enums1 = Default::default();
        let objs = Default::default();
        let fm = DefaultsValidator::new(&enums1, &objs);
        fm.validate_prop_defaults(&prop)?;
        prop.default = json!(100);

        fm.validate_prop_defaults(&prop).expect_err(
            "Should error out, default is number when it should be a string (bundleImage string)",
        );
        Ok(())
    }

    #[test]
    fn test_validate_prop_defaults_bundle_text() -> Result<()> {
        let mut prop = PropDef::new("key", TypeRef::BundleText, json!("BundledText"));
        let enums1 = Default::default();
        let objs = Default::default();
        let fm = DefaultsValidator::new(&enums1, &objs);
        fm.validate_prop_defaults(&prop)?;
        prop.default = json!(100);

        fm.validate_prop_defaults(&prop).expect_err(
            "Should error out, default is number when it should be a string (bundleText string)",
        );
        Ok(())
    }

    #[test]
    fn test_validate_prop_defaults_option_null() -> Result<()> {
        let mut prop = PropDef::new(
            "key",
            TypeRef::Option(Box::new(TypeRef::Boolean)),
            json!(null),
        );
        let enums1 = Default::default();
        let objs = Default::default();
        let fm = DefaultsValidator::new(&enums1, &objs);
        fm.validate_prop_defaults(&prop)?;
        prop.default = json!(100);

        fm.validate_prop_defaults(&prop).expect_err(
            "Should error out, default is number when it should be a boolean (Optional boolean)",
        );
        Ok(())
    }

    #[test]
    fn test_validate_prop_defaults_nested_options() -> Result<()> {
        let prop = PropDef::new(
            "key",
            TypeRef::Option(Box::new(TypeRef::Option(Box::new(TypeRef::Boolean)))),
            json!(true),
        );
        let enums1 = Default::default();
        let objs = Default::default();
        let fm = DefaultsValidator::new(&enums1, &objs);
        fm.validate_prop_defaults(&prop)
            .expect_err("Should error out since we have a nested option");
        Ok(())
    }

    #[test]
    fn test_validate_prop_defaults_option_non_null() -> Result<()> {
        let mut prop = PropDef::new(
            "key",
            TypeRef::Option(Box::new(TypeRef::Boolean)),
            json!(true),
        );
        let enums1 = Default::default();
        let objs = Default::default();
        let fm = DefaultsValidator::new(&enums1, &objs);
        fm.validate_prop_defaults(&prop)?;

        prop.default = json!(100);
        fm.validate_prop_defaults(&prop).expect_err(
            "Should error out, default is number when it should be a boolean (Optional boolean)",
        );
        Ok(())
    }

    #[test]
    fn test_validate_prop_defaults_enum() -> Result<()> {
        let mut prop = PropDef::new("key", TypeRef::Enum("ButtonColor".into()), json!("blue"));

        let enums1 = enums();
        let objs = Default::default();
        let fm = DefaultsValidator::new(&enums1, &objs);
        fm.validate_prop_defaults(&prop)?;
        prop.default = json!("green");

        fm.validate_prop_defaults(&prop)?;
        prop.default = json!("not a valid color");

        fm.validate_prop_defaults(&prop)
            .expect_err("Should error out since default is not a valid enum variant");
        Ok(())
    }

    #[test]
    fn test_validate_prop_defaults_enum_map() -> Result<()> {
        let mut prop = PropDef::new(
            "key",
            TypeRef::EnumMap(
                Box::new(TypeRef::Enum("ButtonColor".into())),
                Box::new(TypeRef::Int),
            ),
            json!({
                "blue": 1,
                "green": 22,
            }),
        );
        let enums1 = enums();
        let objs = Default::default();
        let fm = DefaultsValidator::new(&enums1, &objs);
        fm.validate_prop_defaults(&prop)?;
        prop.default = json!({
            "blue": 1,
        });
        fm.validate_prop_defaults(&prop)
            .expect_err("Should error out because the enum map is missing the green key");

        prop.default = json!({
            "blue": 1,
            "green": 22,
            "red": 3,
        });
        fm.validate_prop_defaults(&prop).expect_err("Should error out because the default includes an extra key that is not a variant of the enum (red)");
        Ok(())
    }

    #[test]
    fn test_validate_prop_defaults_string_map() -> Result<()> {
        let mut prop = PropDef::new(
            "key",
            TypeRef::StringMap(Box::new(TypeRef::Int)),
            json!({
                "blue": 1,
                "green": 22,
            }),
        );
        let enums1 = Default::default();
        let objs = Default::default();
        let fm = DefaultsValidator::new(&enums1, &objs);
        fm.validate_prop_defaults(&prop)?;
        prop.default = json!({
            "blue": 1,
        });
        fm.validate_prop_defaults(&prop)?;

        prop.default = json!({
            "blue": 1,
            "green": 22,
            "red": 3,
            "white": "AHA not a number"
        });
        fm.validate_prop_defaults(&prop).expect_err("Should error out because the string map includes a value that is not an int as defined by the TypeRef");
        Ok(())
    }

    #[test]
    fn test_validate_prop_defaults_list() -> Result<()> {
        let mut prop = PropDef::new(
            "key",
            TypeRef::List(Box::new(TypeRef::Int)),
            json!([1, 3, 100]),
        );
        let enums1 = Default::default();
        let objs = Default::default();
        let fm = DefaultsValidator::new(&enums1, &objs);
        fm.validate_prop_defaults(&prop)?;

        prop.default = json!([1, 2, "oops"]);
        fm.validate_prop_defaults(&prop)
            .expect_err("Should error out because one of the values in the array is not an int");
        Ok(())
    }

    #[test]
    fn test_validate_prop_defaults_object() -> Result<()> {
        let mut prop = PropDef::new(
            "key",
            TypeRef::Object("SampleObj".into()),
            json!({
                "int": 1,
                "string": "bobo",
                "enum": "green",
                "list": [true, false, true],
                "nestedObj": {
                    "enumMap": {
                        "blue": 1,
                        "green": 2,
                    }
                },
                "optional": 2,
            }),
        );

        let enums1 = enums();
        let objs = objects();
        let fm = DefaultsValidator::new(&enums1, &objs);
        fm.validate_prop_defaults(&prop)?;

        prop.default = json!({
            "int": 1,
            "string": "bobo",
            "enum": "green",
            "list": [true, false, true],
            "nestedObj": {
                "enumMap": {
                    "blue": 1,
                    "green": "Wrong type!"
                }
            }
        });
        fm.validate_prop_defaults(&prop).expect_err(
            "Should error out because the nested object has an enumMap with the wrong type",
        );

        prop.default = json!({
            "int": 1,
            "string": "bobo",
            "enum": "green",
            "list": [true, false, true],
            "nestedObj": {
                "enumMap": {
                    "blue": 1,
                    "green": 2,
                }
            },
            "optional": 3,
            "extra-property": 2
        });
        fm.validate_prop_defaults(&prop)
            .expect_err("Should error out because the object has an extra property");

        // This test is missing a `list` property. But that's ok, because we'll get it from the object definition.
        prop.default = json!({
            "int": 1,
            "string": "bobo",
            "enum": "green",
            "nestedObj": {
                "enumMap": {
                    "blue": 1,
                    "green": 2,
                }
            },
            "optional": 2,
        });
        fm.validate_prop_defaults(&prop)?;

        prop.default = json!({
            "int": 1,
            "string": "bobo",
            "enum": "green",
            "list": [true, false, true],
            "nestedObj": {
                "enumMap": {
                    "blue": 1,
                    "green": 2,
                }
            },
        });

        // OK, because we are missing `optional` which is optional anyways
        fm.validate_prop_defaults(&prop)?;
        Ok(())
    }

    #[test]
    fn test_validate_prop_defaults_enum_map_optional() -> Result<()> {
        let prop = PropDef::new(
            "key",
            TypeRef::EnumMap(
                Box::new(TypeRef::Enum("ButtonColor".into())),
                Box::new(TypeRef::Option(Box::new(TypeRef::Int))),
            ),
            json!({
                "blue": 1,
            }),
        );
        let enums1 = enums();
        let objs = Default::default();
        let fm = DefaultsValidator::new(&enums1, &objs);
        // OK because the value is optional, and thus it's okay if it's missing (green is missing from the default)
        fm.validate_prop_defaults(&prop)?;
        Ok(())
    }
}
