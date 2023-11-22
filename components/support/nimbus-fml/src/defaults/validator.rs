/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::{did_you_mean, FMLError};
use crate::intermediate_representation::{FeatureDef, PropDef, TypeRef};
use crate::{
    error::Result,
    intermediate_representation::{EnumDef, ObjectDef},
};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};

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

        let string_aliases = feature_def.get_string_aliases();
        let mut errors = Default::default();
        for prop in &feature_def.props {
            let path = format!("features/{}.{}", feature_def.name, prop.name);
            let error_path = vec![prop.name.to_string()];

            self.validate_string_aliases(
                path.as_str(),
                &error_path,
                &prop.typ,
                &prop.default,
                &string_aliases,
                &prop.string_alias,
                &mut errors,
            );
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.pop().unwrap())
        }
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

    #[allow(clippy::too_many_arguments)]
    fn validate_string_aliases(
        &self,
        path: &str,
        error_path: &[String],
        typ: &TypeRef,
        value: &Value,
        defn: &HashMap<&str, &PropDef>,
        skip: &Option<TypeRef>,
        errors: &mut Vec<FMLError>,
    ) {
        // As an optimization (to stop validating the definition against itself),
        // we want to skip validation on the `skip` type ref: this is only set by the property defining
        // a string-alias.
        let should_validate = |v: &TypeRef| -> bool { skip.as_ref() != Some(v) };
        match (typ, value) {
            (TypeRef::StringAlias(_), Value::String(s)) => {
                check_string_aliased_property(path, error_path, typ, s, defn, errors)
            }
            (TypeRef::Option(_), &Value::Null) => (),
            (TypeRef::Option(inner), _) => {
                self.validate_string_aliases(path, error_path, inner, value, defn, skip, errors)
            }
            (TypeRef::List(inner), Value::Array(array)) => {
                if should_validate(inner) {
                    for value in array {
                        self.validate_string_aliases(
                            path, error_path, inner, value, defn, skip, errors,
                        );
                    }
                }
            }
            (TypeRef::EnumMap(key_type, value_type), Value::Object(map)) => {
                if should_validate(key_type) && matches!(**key_type, TypeRef::StringAlias(_)) {
                    for value in map.keys() {
                        check_string_aliased_property(
                            path, error_path, key_type, value, defn, errors,
                        );
                    }
                }

                if should_validate(value_type) {
                    for (key, value) in map {
                        let path = format!("{path}['{key}']");
                        let error_path = append_quoted(error_path, key);

                        self.validate_string_aliases(
                            &path,
                            &error_path,
                            value_type,
                            value,
                            defn,
                            skip,
                            errors,
                        );
                    }
                }
            }
            (TypeRef::StringMap(vt), Value::Object(map)) => {
                if should_validate(vt) {
                    for (key, value) in map {
                        let path = format!("{path}['{key}']");
                        let error_path = append1(error_path, key);

                        self.validate_string_aliases(
                            &path,
                            &error_path,
                            vt,
                            value,
                            defn,
                            skip,
                            errors,
                        );
                    }
                }
            }
            (TypeRef::Object(obj_nm), Value::Object(map)) => {
                let path = format!("{path}#{obj_nm}");
                let error_path = append1(error_path, "{");
                let obj_def = self.get_object(obj_nm).unwrap();

                for prop in &obj_def.props {
                    let prop_nm = &prop.name;
                    if let Some(value) = map.get(prop_nm) {
                        let path = format!("{path}.{prop_nm}");
                        let error_path = append_quoted(&error_path, prop_nm);
                        self.validate_string_aliases(
                            &path,
                            &error_path,
                            &prop.typ,
                            value,
                            defn,
                            // string-alias definitions aren't allowed in Object definitions,
                            // so `skip` is None.
                            &None,
                            errors,
                        );
                    } else {
                        // There is no value in the map, so we need to validate the
                        // default.
                        let mut suberrors = Default::default();
                        self.validate_string_aliases(
                            "",
                            Default::default(),
                            &prop.typ,
                            &prop.default,
                            defn,
                            &None,
                            &mut suberrors,
                        );

                        // If the default is invalid, then it doesn't really matter
                        // what the error is, we can just error out.
                        if !suberrors.is_empty() {
                            errors.push(FMLError::FeatureValidationError {
                                literals: error_path.clone(),
                                path: path.clone(),
                                message: format!(
                                    "A valid value for {prop_nm} of type {} is missing",
                                    &prop.typ
                                ),
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn check_string_aliased_property(
    path: &str,
    error_path: &[String],
    alias_type: &TypeRef,
    value: &str,
    defn: &HashMap<&str, &PropDef>,
    errors: &mut Vec<FMLError>,
) {
    if let TypeRef::StringAlias(alias_nm) = alias_type {
        if let Some(prop) = defn.get(alias_nm.as_str()) {
            if !validate_string_alias_value(value, alias_type, &prop.typ, &prop.default) {
                let mut valid = Default::default();
                collect_string_alias_values(alias_type, &prop.typ, &prop.default, &mut valid);
                errors.push(FMLError::FeatureValidationError {
                    literals: append_quoted(error_path, value),
                    path: path.to_string(),
                    message: format!(
                        "Invalid value \"{value}\" for type {alias_nm}{}",
                        did_you_mean(valid)
                    ),
                })
            }
        }
    }
}

/// Takes
/// - a string-alias type, StringAlias("TeammateName") / TeamMateName
/// - a type definition of a wider collection of teammates: e.g. Map<TeamMateName, TeamMate>
/// - an a value for the collection of teammates: e.g. {"Alice": {}, "Bonnie": {}, "Charlie": {}, "Dawn"}
///
/// and fills a hash set with the full set of TeamMateNames, in this case: ["Alice", "Bonnie", "Charlie", "Dawn"]
fn collect_string_alias_values(
    alias_type: &TypeRef,
    def_type: &TypeRef,
    def_value: &Value,
    set: &mut HashSet<String>,
) {
    match (def_type, def_value) {
        (TypeRef::StringAlias(_), Value::String(s)) if alias_type == def_type => {
            set.insert(s.clone());
        }
        (TypeRef::Option(dt), dv) if dv != &Value::Null => {
            collect_string_alias_values(alias_type, dt, dv, set);
        }
        (TypeRef::EnumMap(kt, _), Value::Object(map)) if alias_type == &**kt => {
            set.extend(map.keys().cloned());
        }
        (TypeRef::EnumMap(_, vt), Value::Object(map))
        | (TypeRef::StringMap(vt), Value::Object(map)) => {
            for item in map.values() {
                collect_string_alias_values(alias_type, vt, item, set);
            }
        }
        (TypeRef::List(vt), Value::Array(array)) => {
            for item in array {
                collect_string_alias_values(alias_type, vt, item, set);
            }
        }
        _ => {}
    }
}

/// Takes
/// - a string value e.g. "Alice"
/// - a string-alias type, StringAlias("TeamMateName") / TeamMateName
/// - a type definition of a wider collection of teammates: e.g. List<TeamMateName>
/// - an a value for the collection of teammates: e.g. ["Alice", "Bonnie", "Charlie", "Dawn"]
///
/// Given the args, returns a boolean: is the string value in the collection?
///
/// This should work with arbitrary collection types, e.g.
/// - TeamMate,
/// - Option<TeamMate>,
/// - List<TeamMate>,
/// - Map<TeamMate, _>
/// - Map<_, TeamMate>
///
/// and any arbitrary nesting of the collection types.
fn validate_string_alias_value(
    value: &str,
    alias_type: &TypeRef,
    def_type: &TypeRef,
    def_value: &Value,
) -> bool {
    match (def_type, def_value) {
        (TypeRef::StringAlias(_), Value::String(s)) if alias_type == def_type => value == s,

        (TypeRef::Option(dt), dv) if dv != &Value::Null => {
            validate_string_alias_value(value, alias_type, dt, dv)
        }
        (TypeRef::EnumMap(kt, _), Value::Object(map)) if alias_type == &**kt => {
            map.contains_key(value)
        }
        (TypeRef::EnumMap(_, vt), Value::Object(map))
        | (TypeRef::StringMap(vt), Value::Object(map)) => {
            let mut found = false;
            for item in map.values() {
                if validate_string_alias_value(value, alias_type, vt, item) {
                    found = true;
                    break;
                }
            }
            found
        }
        (TypeRef::List(k), Value::Array(array)) => {
            let mut found = false;
            for item in array {
                if validate_string_alias_value(value, alias_type, k, item) {
                    found = true;
                    break;
                }
            }
            found
        }

        _ => false,
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

    use crate::intermediate_representation::PropDef;

    use super::*;

    impl DefaultsValidator<'_> {
        fn validate_prop_defaults(&self, prop: &PropDef) -> Result<()> {
            let error_path = Default::default();
            self.validate_types(prop.name.as_str(), &error_path, &prop.typ, &prop.default)
        }
    }

    fn enums() -> BTreeMap<String, EnumDef> {
        let enum_ = EnumDef::new("ButtonColor", &["blue", "green"]);

        EnumDef::into_map(&[enum_])
    }

    fn objects() -> BTreeMap<String, ObjectDef> {
        let obj1 = ObjectDef::new(
            "SampleObj",
            &[
                PropDef::new("int", &TypeRef::Int, &json!(1)),
                PropDef::new("string", &TypeRef::String, &json!("a string")),
                PropDef::new("enum", &TypeRef::Enum("ButtonColor".into()), &json!("blue")),
                PropDef::new(
                    "list",
                    &TypeRef::List(Box::new(TypeRef::Boolean)),
                    &json!([true, false]),
                ),
                PropDef::new(
                    "optional",
                    &TypeRef::Option(Box::new(TypeRef::Int)),
                    &json!(null),
                ),
                PropDef::new(
                    "nestedObj",
                    &TypeRef::Object("NestedObject".into()),
                    &json!({
                        "enumMap": {
                            "blue": 1,
                        },
                    }),
                ),
            ],
        );

        let obj2 = ObjectDef::new(
            "NestedObject",
            &[PropDef::new(
                "enumMap",
                &TypeRef::EnumMap(
                    Box::new(TypeRef::Enum("ButtonColor".into())),
                    Box::new(TypeRef::Int),
                ),
                &json!({
                    "blue": 4,
                    "green": 2,
                }),
            )],
        );
        ObjectDef::into_map(&[obj1, obj2])
    }

    #[test]
    fn test_validate_prop_defaults_string() -> Result<()> {
        let mut prop = PropDef::new("key", &TypeRef::String, &json!("default!"));
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
        let mut prop = PropDef::new("key", &TypeRef::Int, &json!(100));
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
        let mut prop = PropDef::new("key", &TypeRef::Boolean, &json!(true));
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
        let mut prop = PropDef::new("key", &TypeRef::BundleImage, &json!("IconBlue"));
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
        let mut prop = PropDef::new("key", &TypeRef::BundleText, &json!("BundledText"));
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
            &TypeRef::Option(Box::new(TypeRef::Boolean)),
            &json!(null),
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
            &TypeRef::Option(Box::new(TypeRef::Option(Box::new(TypeRef::Boolean)))),
            &json!(true),
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
            &TypeRef::Option(Box::new(TypeRef::Boolean)),
            &json!(true),
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
        let mut prop = PropDef::new("key", &TypeRef::Enum("ButtonColor".into()), &json!("blue"));

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
            &TypeRef::EnumMap(
                Box::new(TypeRef::Enum("ButtonColor".into())),
                Box::new(TypeRef::Int),
            ),
            &json!({
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
            &TypeRef::StringMap(Box::new(TypeRef::Int)),
            &json!({
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
            &TypeRef::List(Box::new(TypeRef::Int)),
            &json!([1, 3, 100]),
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
            &TypeRef::Object("SampleObj".into()),
            &json!({
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
            &TypeRef::EnumMap(
                Box::new(TypeRef::Enum("ButtonColor".into())),
                Box::new(TypeRef::Option(Box::new(TypeRef::Int))),
            ),
            &json!({
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

#[cfg(test)]
mod string_alias {

    use super::*;
    use serde_json::json;

    fn test_set(alias_type: &TypeRef, def_type: &TypeRef, def_value: &Value, set: &[&str]) {
        let mut observed = Default::default();
        collect_string_alias_values(alias_type, def_type, def_value, &mut observed);

        let expected: HashSet<_> = set.iter().map(|s| s.to_string()).collect();
        assert_eq!(expected, observed);
    }

    // Does this string belong in the type definition?
    #[test]
    fn test_validate_value() -> Result<()> {
        let sa = TypeRef::StringAlias("Name".to_string());

        // type definition is Name
        let def = sa.clone();
        let value = json!("yes");
        assert!(validate_string_alias_value("yes", &sa, &def, &value));
        assert!(!validate_string_alias_value("no", &sa, &def, &value));
        test_set(&sa, &def, &value, &["yes"]);

        // type definition is Name?
        let def = TypeRef::Option(Box::new(sa.clone()));
        let value = json!("yes");
        assert!(validate_string_alias_value("yes", &sa, &def, &value));
        assert!(!validate_string_alias_value("no", &sa, &def, &value));
        test_set(&sa, &def, &value, &["yes"]);

        let value = json!(null);
        assert!(!validate_string_alias_value("no", &sa, &def, &value));
        test_set(&sa, &def, &value, &[]);

        // type definition is Map<Name, Boolean>
        let def = TypeRef::EnumMap(Box::new(sa.clone()), Box::new(TypeRef::Boolean));
        let value = json!({
            "yes": true,
            "YES": false,
        });
        assert!(validate_string_alias_value("yes", &sa, &def, &value));
        assert!(validate_string_alias_value("YES", &sa, &def, &value));
        assert!(!validate_string_alias_value("no", &sa, &def, &value));
        test_set(&sa, &def, &value, &["yes", "YES"]);

        // type definition is Map<String, Name>
        let def = TypeRef::EnumMap(Box::new(TypeRef::String), Box::new(sa.clone()));
        let value = json!({
            "ok": "yes",
            "OK": "YES",
        });
        assert!(validate_string_alias_value("yes", &sa, &def, &value));
        assert!(validate_string_alias_value("YES", &sa, &def, &value));
        assert!(!validate_string_alias_value("no", &sa, &def, &value));
        test_set(&sa, &def, &value, &["yes", "YES"]);

        // type definition is List<String>
        let def = TypeRef::List(Box::new(sa.clone()));
        let value = json!(["yes", "YES"]);
        assert!(validate_string_alias_value("yes", &sa, &def, &value));
        assert!(validate_string_alias_value("YES", &sa, &def, &value));
        assert!(!validate_string_alias_value("no", &sa, &def, &value));
        test_set(&sa, &def, &value, &["yes", "YES"]);

        // type definition is List<Map<String, Name>>
        let def = TypeRef::List(Box::new(TypeRef::StringMap(Box::new(sa.clone()))));
        let value = json!([{"y": "yes"}, {"Y": "YES"}]);
        assert!(validate_string_alias_value("yes", &sa, &def, &value));
        assert!(validate_string_alias_value("YES", &sa, &def, &value));
        assert!(!validate_string_alias_value("no", &sa, &def, &value));
        test_set(&sa, &def, &value, &["yes", "YES"]);

        // type definition is Map<String, List<Name>>
        let def = TypeRef::StringMap(Box::new(TypeRef::List(Box::new(sa.clone()))));
        let value = json!({"y": ["yes"], "Y": ["YES"]});
        assert!(validate_string_alias_value("yes", &sa, &def, &value));
        assert!(validate_string_alias_value("YES", &sa, &def, &value));
        assert!(!validate_string_alias_value("no", &sa, &def, &value));
        test_set(&sa, &def, &value, &["yes", "YES"]);

        Ok(())
    }

    fn objects(nm: &str, props: &[PropDef]) -> BTreeMap<String, ObjectDef> {
        let obj1 = ObjectDef::new(nm, props);
        ObjectDef::into_map(&[obj1])
    }

    fn feature(props: &[PropDef]) -> FeatureDef {
        FeatureDef {
            name: "TestFeature".to_string(),
            props: props.into(),
            ..Default::default()
        }
    }

    #[test]
    fn test_string_alias() -> Result<()> {
        let mate = TypeRef::StringAlias("TeamMate".to_string());
        let the_team = {
            let team = TypeRef::List(Box::new(mate.clone()));
            let value = json!(["Alice", "Bonnie", "Charlie", "Deborah", "Eve"]);

            PropDef::with_string_alias("team", &team, &value, &mate)
        };
        test_with_simple_string_alias(&mate, &the_team)?;
        test_with_objects(&mate, &the_team)?;

        let the_team = {
            let team = TypeRef::EnumMap(Box::new(mate.clone()), Box::new(TypeRef::Boolean));
            let value = json!({"Alice": true, "Bonnie": true, "Charlie": true, "Deborah": true, "Eve": true});

            PropDef::with_string_alias("team", &team, &value, &mate)
        };
        test_with_simple_string_alias(&mate, &the_team)?;
        test_with_objects(&mate, &the_team)?;

        Ok(())
    }

    fn test_with_simple_string_alias(mate: &TypeRef, the_team: &PropDef) -> Result<()> {
        let objs = Default::default();
        let enums = Default::default();
        let validator = DefaultsValidator::new(&enums, &objs);

        // For all these tests, the_team defines the set of strings which are valid TeamMate strings.

        // captain is a TeamMate
        let nm = "captain";
        let t = mate.clone();
        let f = {
            let v = json!("Eve");
            feature(&[the_team.clone(), PropDef::new(nm, &t, &v)])
        };

        validator.validate_feature_def(&f)?;

        let t = mate.clone();
        let f = {
            let v = json!("Nope");
            feature(&[the_team.clone(), PropDef::new(nm, &t, &v)])
        };
        assert!(validator.validate_feature_def(&f).is_err());

        // goalkeeper is an Option<TeamMate>
        let nm = "goalkeeper";
        let t = TypeRef::Option(Box::new(mate.clone()));
        let f = {
            let v = json!(null);
            feature(&[the_team.clone(), PropDef::new(nm, &t, &v)])
        };
        validator.validate_feature_def(&f)?;

        let f = {
            let v = json!("Charlie");
            feature(&[the_team.clone(), PropDef::new(nm, &t, &v)])
        };
        validator.validate_feature_def(&f)?;

        let f = {
            let v = json!("Nope");
            feature(&[the_team.clone(), PropDef::new(nm, &t, &v)])
        };
        assert!(validator.validate_feature_def(&f).is_err());

        // defenders are List<TeamMate>
        let nm = "defenders";
        let t = TypeRef::List(Box::new(mate.clone()));

        let f = {
            let v = json!([]);
            feature(&[the_team.clone(), PropDef::new(nm, &t, &v)])
        };
        validator.validate_feature_def(&f)?;

        let f = {
            let v = json!(["Alice", "Charlie"]);
            feature(&[the_team.clone(), PropDef::new(nm, &t, &v)])
        };
        validator.validate_feature_def(&f)?;

        let f = {
            let v = json!(["Alice", "Nope"]);
            feature(&[the_team.clone(), PropDef::new(nm, &t, &v)])
        };
        assert!(validator.validate_feature_def(&f).is_err());

        // injury-status are Map<TeamMate, Boolean>
        let nm = "injury-status";
        let t = TypeRef::EnumMap(Box::new(mate.clone()), Box::new(TypeRef::Boolean));
        let f = {
            let v = json!({"Bonnie": false, "Deborah": true});
            feature(&[the_team.clone(), PropDef::new(nm, &t, &v)])
        };
        validator.validate_feature_def(&f)?;

        let f = {
            let v = json!({"Bonnie": false, "Nope": true});
            feature(&[the_team.clone(), PropDef::new(nm, &t, &v)])
        };
        assert!(validator.validate_feature_def(&f).is_err());

        // positions are Map<PositionName, List<TeamMate>>
        let nm = "positions";
        let position = TypeRef::StringAlias("PositionName".to_string());
        let t = TypeRef::EnumMap(
            Box::new(position.clone()),
            Box::new(TypeRef::List(Box::new(mate.clone()))),
        );
        let f = {
            let v = json!({"DEFENDER": ["Bonnie", "Charlie"], "MIDFIELD": ["Alice", "Deborah"], "FORWARD": ["Eve"]});
            feature(&[
                the_team.clone(),
                PropDef::with_string_alias(nm, &t, &v, &position),
            ])
        };
        validator.validate_feature_def(&f)?;

        let f = {
            let v = json!({"DEFENDER": ["Bonnie", "Charlie"], "MIDFIELD": ["Alice", "Deborah"], "STRIKER": ["Eve"]});
            feature(&[
                the_team.clone(),
                PropDef::with_string_alias(nm, &t, &v, &position),
            ])
        };
        validator.validate_feature_def(&f)?;

        let f = {
            let v = json!({"DEFENDER": ["Bonnie", "Charlie"], "MIDFIELD": ["Nope", "Deborah"], "STRIKER": ["Eve"]});
            feature(&[
                the_team.clone(),
                PropDef::with_string_alias(nm, &t, &v, &position),
            ])
        };
        assert!(validator.validate_feature_def(&f).is_err());
        Ok(())
    }

    fn test_with_objects(mate: &TypeRef, the_team: &PropDef) -> Result<()> {
        let position = TypeRef::StringAlias("PositionName".to_string());
        let positions = {
            let nm = "positions";
            let t = TypeRef::EnumMap(
                Box::new(position.clone()),
                Box::new(TypeRef::List(Box::new(mate.clone()))),
            );
            let v = json!({"DEFENDER": ["Bonnie", "Charlie"], "MIDFIELD": ["Alice", "Deborah"], "FORWARD": ["Eve"]});
            PropDef::with_string_alias(nm, &t, &v, &position)
        };

        let objects = objects(
            "Player",
            &[
                PropDef::new("name", mate, &json!("Untested")),
                PropDef::new("position", &position, &json!("Untested")),
            ],
        );
        let enums = Default::default();
        let validator = DefaultsValidator::new(&enums, &objects);

        // newest-player: Player
        let nm = "newest-player";
        let t = TypeRef::Object("Player".to_string());
        let f = {
            let v = json!({"name": "Eve", "position": "FORWARD"});
            feature(&[
                the_team.clone(),
                positions.clone(),
                PropDef::new(nm, &t, &v),
            ])
        };
        validator.validate_feature_def(&f)?;

        let f = {
            let v = json!({"name": "Nope", "position": "FORWARD"});
            feature(&[
                the_team.clone(),
                positions.clone(),
                PropDef::new(nm, &t, &v),
            ])
        };
        assert!(validator.validate_feature_def(&f).is_err());

        // positions: List<PositionName>
        // players: Map<TeamMateName, Player>
        let positions = {
            let t = TypeRef::List(Box::new(position.clone()));
            let v = json!(["FORWARD", "DEFENDER"]);
            PropDef::with_string_alias("positions", &t, &v, &position)
        };
        let nm = "players";
        let t = TypeRef::EnumMap(
            Box::new(mate.clone()),
            Box::new(TypeRef::Object("Player".to_string())),
        );
        let f = {
            let v = json!({ "Eve": {"name": "Eve", "position": "FORWARD"}});
            feature(&[
                positions.clone(),
                PropDef::with_string_alias(nm, &t, &v, mate),
            ])
        };
        validator.validate_feature_def(&f)?;

        let f = {
            let v = json!({ "Nope": {"name": "Eve", "position": "FORWARD"}});
            feature(&[
                positions.clone(),
                PropDef::with_string_alias(nm, &t, &v, mate),
            ])
        };
        assert!(validator.validate_feature_def(&f).is_err());

        Ok(())
    }
}
