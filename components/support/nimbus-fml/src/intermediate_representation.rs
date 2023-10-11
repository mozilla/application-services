/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use crate::defaults_merger::DefaultsMerger;
use crate::error::FMLError::InvalidFeatureError;
use crate::error::{did_you_mean, FMLError, Result};
use crate::frontend::AboutBlock;
use crate::util::loaders::FilePath;
use anyhow::{bail, Error, Result as AnyhowResult};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Display;

#[derive(Eq, PartialEq, Hash, Debug, Clone)]
pub enum TargetLanguage {
    Kotlin,
    Swift,
    IR,
    ExperimenterYAML,
    ExperimenterJSON,
}

impl TargetLanguage {
    pub fn extension(&self) -> &str {
        match self {
            TargetLanguage::Kotlin => "kt",
            TargetLanguage::Swift => "swift",
            TargetLanguage::IR => "fml.json",
            TargetLanguage::ExperimenterJSON => "json",
            TargetLanguage::ExperimenterYAML => "yaml",
        }
    }

    pub fn from_extension(path: &str) -> AnyhowResult<TargetLanguage> {
        if let Some((_, extension)) = path.rsplit_once('.') {
            extension.try_into()
        } else {
            bail!("Unknown or unsupported target language: \"{}\"", path)
        }
    }
}

impl TryFrom<&str> for TargetLanguage {
    type Error = Error;
    fn try_from(value: &str) -> AnyhowResult<Self> {
        Ok(match value.to_ascii_lowercase().as_str() {
            "kotlin" | "kt" | "kts" => TargetLanguage::Kotlin,
            "swift" => TargetLanguage::Swift,
            "fml.json" => TargetLanguage::IR,
            "yaml" => TargetLanguage::ExperimenterYAML,
            "json" => TargetLanguage::ExperimenterJSON,
            _ => bail!("Unknown or unsupported target language: \"{}\"", value),
        })
    }
}

/// The `TypeRef` enum defines a reference to a type.
///
/// Other types will be defined in terms of these enum values.
///
/// They represent the types available via the current `Variables` API—
/// some primitives and structural types— and can be represented by
/// Kotlin, Swift and JSON Schema.
///
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Hash, Eq)]
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
    // We can coerce the String keys into Enums, so this represents that.
    EnumMap(Box<TypeRef>, Box<TypeRef>),

    List(Box<TypeRef>),
    Option(Box<TypeRef>),
}

impl Display for TypeRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::String => f.write_str("String"),
            Self::Int => f.write_str("Int"),
            Self::Boolean => f.write_str("Boolean"),
            Self::BundleImage(_) => f.write_str("Image"),
            Self::BundleText(_) => f.write_str("Text"),
            Self::Enum(v) => f.write_str(v),
            Self::Object(v) => f.write_str(v),
            Self::Option(v) => f.write_fmt(format_args!("Option<{v}>")),
            Self::List(v) => f.write_fmt(format_args!("List<{v}>")),
            Self::StringMap(v) => f.write_fmt(format_args!("Map<String, {v}>")),
            Self::EnumMap(k, v) => f.write_fmt(format_args!("Map<{k}, {v}>")),
        }
    }
}

impl TypeRef {
    pub(crate) fn supports_prefs(&self) -> bool {
        match self {
            Self::Boolean | Self::String | Self::Int | Self::BundleText(_) => true,
            // There may be a chance that we can get Self::Option to work, but not at this time.
            // This may be done by adding a branch to this match and adding a `preference_getter` to
            // the `OptionalCodeType`.
            _ => false,
        }
    }
}

/**
 * An identifier derived from a `FilePath` of a top-level or importable FML file.
 *
 * An FML module is the conceptual FML file (and included FML files) that a single
 * Kotlin or Swift file. It can be imported by other FML modules.
 *
 * It is somewhat distinct from the `FilePath` enum for three reasons:
 *
 * - a file path can specify a non-canonical representation of the path
 * - a file path is difficult to serialize/deserialize
 * - a module identifies the cluster of FML files that map to a single generated
 * Kotlin or Swift file; this difference can be seen as: files can be included,
 * modules can be imported.
 */
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Hash, Clone, Serialize, Deserialize)]
pub enum ModuleId {
    Local(String),
    Remote(String),
}

impl Default for ModuleId {
    fn default() -> Self {
        Self::Local("none".to_string())
    }
}

impl TryFrom<&FilePath> for ModuleId {
    type Error = FMLError;
    fn try_from(path: &FilePath) -> Result<Self> {
        Ok(match path {
            FilePath::Local(p) => {
                // We do this map_err here because the IO Error message that comes out of `canonicalize`
                // doesn't include the problematic file path.
                let p = p.canonicalize().map_err(|e| {
                    FMLError::InvalidPath(format!("{}: {}", e, p.as_path().display()))
                })?;
                ModuleId::Local(p.display().to_string())
            }
            FilePath::Remote(u) => ModuleId::Remote(u.to_string()),
        })
    }
}

impl Display for ModuleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ModuleId::Local(s) | ModuleId::Remote(s) => s,
        })
    }
}

pub trait TypeFinder {
    fn all_types(&self) -> HashSet<TypeRef> {
        let mut types = HashSet::new();
        self.find_types(&mut types);
        types
    }

    fn find_types(&self, types: &mut HashSet<TypeRef>);
}

impl TypeFinder for TypeRef {
    fn find_types(&self, types: &mut HashSet<TypeRef>) {
        if types.insert(self.clone()) {
            match self {
                TypeRef::List(v) | TypeRef::Option(v) | TypeRef::StringMap(v) => {
                    v.find_types(types)
                }
                TypeRef::EnumMap(k, v) => {
                    k.find_types(types);
                    v.find_types(types);
                }
                _ => {}
            }
        }
    }
}

pub(crate) type StringId = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct FeatureManifest {
    #[serde(skip)]
    pub(crate) id: ModuleId,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub(crate) channel: Option<String>,

    #[serde(rename = "enums")]
    #[serde(default)]
    pub(crate) enum_defs: BTreeMap<String, EnumDef>,
    #[serde(rename = "objects")]
    #[serde(default)]
    pub(crate) obj_defs: BTreeMap<String, ObjectDef>,
    #[serde(rename = "features")]
    pub(crate) feature_defs: BTreeMap<String, FeatureDef>,
    #[serde(default)]
    pub(crate) about: AboutBlock,

    #[serde(default)]
    pub(crate) imported_features: HashMap<ModuleId, BTreeSet<String>>,

    #[serde(default)]
    pub(crate) all_imports: HashMap<ModuleId, FeatureManifest>,
}

impl TypeFinder for FeatureManifest {
    fn find_types(&self, types: &mut HashSet<TypeRef>) {
        for e in self.enum_defs.values() {
            e.find_types(types);
        }
        for o in self.iter_object_defs() {
            o.find_types(types);
        }
        for f in self.iter_feature_defs() {
            f.find_types(types);
        }
    }
}

#[cfg(test)]
impl FeatureManifest {
    pub(crate) fn add_feature(&mut self, feature: FeatureDef) {
        self.feature_defs.insert(feature.name(), feature);
    }
}

impl FeatureManifest {
    pub(crate) fn new(
        id: ModuleId,
        channel: Option<&str>,
        features: BTreeMap<String, FeatureDef>,
        enums: BTreeMap<String, EnumDef>,
        objects: BTreeMap<String, ObjectDef>,
        about: AboutBlock,
    ) -> Self {
        Self {
            id,
            channel: channel.map(str::to_string),
            about,
            enum_defs: enums,
            obj_defs: objects,
            feature_defs: features,

            ..Default::default()
        }
    }

    #[allow(unused)]
    pub(crate) fn validate_manifest_for_lang(&self, lang: &TargetLanguage) -> Result<()> {
        if !&self.about.supports(lang) {
            return Err(FMLError::ValidationError(
                "about".to_string(),
                format!(
                    "Manifest file {file} is unable to generate {lang} files",
                    file = &self.id,
                    lang = &lang.extension(),
                ),
            ));
        }
        for child in self.all_imports.values() {
            child.validate_manifest_for_lang(lang)?;
        }
        Ok(())
    }

    pub fn validate_manifest(&self) -> Result<()> {
        // We first validate that each enum_def has a unique name.
        // TODO: We repeat this check three times, it should be its
        // own generic helper
        let mut enum_names = HashSet::new();
        self.validate_enum_defs(&mut enum_names)?;
        // We then validate that each obj_defs also has a unique name.
        let mut obj_names = HashSet::new();
        self.validate_obj_defs(&mut obj_names)?;

        // We then validate that each feature_def has a unique name.
        let mut feature_names = HashSet::new();
        self.validate_feature_defs(&mut feature_names)?;

        // We then validate that each type_ref is valid
        for feature_def in self.iter_feature_defs() {
            for prop in &feature_def.props {
                let path = format!("features/{}.{}", &feature_def.name, &prop.name);
                Self::validate_type_ref(&path, &prop.typ, &enum_names, &obj_names)?;
            }
        }
        self.validate_defaults()?;

        // Validating the imported manifests.
        // This is not only validating the well formed-ness of the imported manifests
        // but also the defaults that are sent into the child manifests.
        for child in self.all_imports.values() {
            child.validate_manifest()?;
        }
        Ok(())
    }

    fn validate_type_ref(
        path: &str,
        type_ref: &TypeRef,
        enum_names: &HashSet<String>,
        obj_names: &HashSet<String>,
    ) -> Result<()> {
        match type_ref {
            TypeRef::Enum(name) => {
                if !enum_names.contains(name) {
                    return Err(FMLError::ValidationError(
                        path.to_string(),
                        format!(
                            "Found enum reference with name: {}, but no definition",
                            name
                        ),
                    ));
                }
                Ok(())
            }
            TypeRef::Object(name) => {
                if !obj_names.contains(name) {
                    return Err(FMLError::ValidationError(
                        path.to_string(),
                        format!(
                            "Found object reference with name: {}, but no definition",
                            name
                        ),
                    ));
                }
                Ok(())
            }
            TypeRef::EnumMap(key_type, value_type) => {
                if let TypeRef::Enum(_) = key_type.as_ref() {
                    Self::validate_type_ref(path, key_type, enum_names, obj_names)?;
                    Self::validate_type_ref(path, value_type, enum_names, obj_names)
                } else {
                    Err(FMLError::ValidationError(
                        path.to_string(),
                        format!("EnumMap key has be an enum, found: {:?}", key_type),
                    ))
                }
            }
            TypeRef::List(list_type) => {
                Self::validate_type_ref(path, list_type, enum_names, obj_names)
            }
            TypeRef::StringMap(value_type) => {
                Self::validate_type_ref(path, value_type, enum_names, obj_names)
            }
            TypeRef::Option(option_type) => {
                if let TypeRef::Option(_) = option_type.as_ref() {
                    Err(FMLError::ValidationError(
                        path.to_string(),
                        "Found nested optional types".into(),
                    ))
                } else {
                    Self::validate_type_ref(path, option_type, enum_names, obj_names)
                }
            }
            _ => Ok(()),
        }
    }

    fn validate_enum_defs(&self, enum_names: &mut HashSet<String>) -> Result<()> {
        for nm in self.enum_defs.keys() {
            if !enum_names.insert(nm.clone()) {
                return Err(FMLError::ValidationError(
                    format!("enums/{}", nm),
                    format!(
                        "EnumDef names must be unique. Found two EnumDefs with the same name: {}",
                        nm
                    ),
                ));
            }
        }
        Ok(())
    }

    fn validate_obj_defs(&self, obj_names: &mut HashSet<String>) -> Result<()> {
        for nm in self.obj_defs.keys() {
            if !obj_names.insert(nm.clone()) {
                return Err(FMLError::ValidationError(
                    format!("objects/{}", nm),
                    format!(
                    "ObjectDef names must be unique. Found two ObjectDefs with the same name: {}",
                    nm
                ),
                ));
            }
        }
        Ok(())
    }

    fn validate_feature_defs(&self, feature_names: &mut HashSet<String>) -> Result<()> {
        for feature_def in self.iter_feature_defs() {
            if !feature_names.insert(feature_def.name.clone()) {
                return Err(FMLError::ValidationError(
                    feature_def.name(),
                    format!(
                    "FeatureDef names must be unique. Found two FeatureDefs with the same name: {}",
                    feature_def.name
                ),
                ));
            }
            // while checking the feature, we also check that each prop is unique within a feature
            let mut prop_names = HashSet::new();
            self.validate_props(feature_def, &mut prop_names)?;
        }
        Ok(())
    }

    fn validate_props(
        &self,
        feature_def: &FeatureDef,
        prop_names: &mut HashSet<String>,
    ) -> Result<()> {
        let path = format!("features/{}", &feature_def.name);
        for prop in &feature_def.props {
            if !prop_names.insert(prop.name.clone()) {
                return Err(FMLError::ValidationError(
                    format!("{}.{}", path, prop.name),
                    format!(
                    "PropDef names must be unique. Found two PropDefs with the same name: {} in the same feature_def: {}",
                    prop.name, feature_def.name
                )));
            }
        }
        Ok(())
    }

    fn validate_defaults(&self) -> Result<()> {
        for object in self.iter_object_defs() {
            for prop in &object.props {
                let path = format!("objects/{}.{}", object.name, prop.name);
                self.validate_prop_defaults(&path, prop)?;
            }
        }
        for feature in self.iter_feature_defs() {
            self.validate_feature_structure(feature)?;
            self.validate_feature_def(feature)?;
        }
        Ok(())
    }

    fn validate_feature_structure(&self, feature_def: &FeatureDef) -> Result<()> {
        for v in &feature_def.props {
            if v.pref_key.is_some() && !v.typ.supports_prefs() {
                return Err(FMLError::ValidationError(
                    format!("features/{}/{}", feature_def.name, v.name),
                    "Pref keys can only be used with Boolean, String, Int and Text variables"
                        .to_string(),
                ));
            }
        }
        Ok(())
    }

    fn validate_feature_def(&self, feature_def: &FeatureDef) -> Result<()> {
        for prop in &feature_def.props {
            let path = format!("features/{}.{}", feature_def.name, prop.name);
            let literals = vec![prop.name.to_string()];
            self.validate_default_by_typ(&path, &literals, &prop.typ, &prop.default)?;
        }
        Ok(())
    }

    fn validate_prop_defaults(&self, path: &str, prop: &PropDef) -> Result<()> {
        let literals = Default::default();
        self.validate_default_by_typ(path, &literals, &prop.typ, &prop.default)
    }

    pub fn validate_default_by_typ(
        &self,
        path: &str,
        literals: &Vec<String>,
        type_ref: &TypeRef,
        default: &Value,
    ) -> Result<()> {
        let add_literals = |new: &[String]| -> Vec<String> {
            let mut clone = literals.clone();
            for s in new {
                clone.push(s.clone());
            }
            clone
        };
        let add_literal = |new: String| -> Vec<String> {
            let mut clone = literals.clone();
            clone.push(new);
            clone
        };
        match (type_ref, default) {
            (TypeRef::Boolean, Value::Bool(_))
            | (TypeRef::BundleImage(_), Value::String(_))
            | (TypeRef::BundleText(_), Value::String(_))
            | (TypeRef::String, Value::String(_))
            | (TypeRef::Int, Value::Number(_))
            | (TypeRef::Option(_), Value::Null) => Ok(()),
            (TypeRef::Option(inner), v) => {
                if let TypeRef::Option(_) = inner.as_ref() {
                    return Err(FMLError::ValidationError(
                        path.to_string(),
                        "Nested options".into(),
                    ));
                }
                self.validate_default_by_typ(path, literals, inner, v)
            }
            (TypeRef::Enum(enum_name), Value::String(s)) => {
                let enum_def = self.find_enum(enum_name).ok_or_else(|| {
                    FMLError::ValidationError(
                        path.to_string(),
                        format!("Type `{enum_name}` is not a type. Perhaps you need to declare an enum of that name.")
                    )
                })?;
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
                    literals: add_literal(format!("\"{s}\"")),
                })
            }
            (TypeRef::EnumMap(enum_type, map_type), Value::Object(map)) => {
                let enum_name = if let TypeRef::Enum(name) = enum_type.as_ref() {
                    name.clone()
                } else {
                    unreachable!()
                };
                // We first validate that the keys of the map cover all all the enum variants, and no more or less
                let enum_def = self.find_enum(&enum_name).ok_or_else(|| {
                    FMLError::ValidationError(
                        path.to_string(),
                        format!("Type `{}` is not a type. Perhaps you need to declare an enum of that name.", enum_name)
                    )
                })?;
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
                            let literals = add_literals(&["{".to_string(), format!("\"{nm}\"")]);
                            self.validate_default_by_typ(&path, &literals, map_type, inner)?;
                            seen.insert(nm);
                        }
                    }
                }

                if !unseen.is_empty() {
                    return Err(FMLError::FeatureValidationError {
                        path: path.to_string(),
                        message: format!("Enum map {enum_name} is missing values for {unseen:?}"),
                        // Can we be more specific that just the opening brace?
                        literals: add_literal("{".to_string()),
                    });
                }
                for map_key in map.keys() {
                    if !seen.contains(map_key) {
                        return Err(FMLError::FeatureValidationError {
                            path: path.to_string(),
                            message: format!("Invalid key \"{map_key}\"{}", did_you_mean(valid)),
                            literals: add_literals(&["{".to_string(), format!("\"{map_key}\"")]),
                        });
                    }
                }
                Ok(())
            }
            (TypeRef::StringMap(map_type), Value::Object(map)) => {
                for (key, value) in map {
                    let path = format!("{path}['{key}']");
                    let literals = add_literals(&["{".to_string(), format!("\"{key}\"")]);
                    self.validate_default_by_typ(&path, &literals, map_type, value)?;
                }
                Ok(())
            }
            (TypeRef::List(list_type), Value::Array(arr)) => {
                let mut literals = add_literal("[".to_string());
                for (index, value) in arr.iter().enumerate() {
                    let path = format!("{path}['{index}']");
                    self.validate_default_by_typ(&path, &literals, list_type, value)?;
                    literals.push(",".to_string());
                }
                Ok(())
            }
            (TypeRef::Object(obj_name), Value::Object(map)) => {
                let obj_def = self.find_object(obj_name).ok_or_else(|| {
                    FMLError::ValidationError(
                        path.to_string(),
                        format!("Object {obj_name} is not defined in the manifest"),
                    )
                })?;
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
                        let literals =
                            add_literals(&["{".to_string(), format!("\"{}\"", &prop.name)]);
                        self.validate_default_by_typ(&path, &literals, &prop.typ, map_val)?;
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
                            literals: add_literal(format!("\"{map_key}\"")),
                        });
                    }
                }

                Ok(())
            }
            _ => Err(FMLError::FeatureValidationError {
                path: path.to_string(),
                message: format!("Mismatch between type {type_ref:?} and default {default}"),
                literals: add_literal(default.to_string()),
            }),
        }
    }

    pub fn iter_enum_defs(&self) -> impl Iterator<Item = &EnumDef> {
        self.enum_defs.values()
    }

    pub fn iter_all_enum_defs(&self) -> impl Iterator<Item = (&FeatureManifest, &EnumDef)> {
        let enums = self.iter_enum_defs().map(move |o| (self, o));
        let imported: Vec<_> = self
            .all_imports
            .values()
            .flat_map(|fm| fm.iter_all_enum_defs())
            .collect();
        enums.chain(imported)
    }

    pub fn iter_object_defs(&self) -> impl Iterator<Item = &ObjectDef> {
        self.obj_defs.values()
    }

    pub fn iter_all_object_defs(&self) -> impl Iterator<Item = (&FeatureManifest, &ObjectDef)> {
        let objects = self.iter_object_defs().map(move |o| (self, o));
        let imported: Vec<_> = self
            .all_imports
            .values()
            .flat_map(|fm| fm.iter_all_object_defs())
            .collect();
        objects.chain(imported)
    }

    pub fn iter_feature_defs(&self) -> impl Iterator<Item = &FeatureDef> {
        self.feature_defs.values()
    }

    pub fn iter_all_feature_defs(&self) -> impl Iterator<Item = (&FeatureManifest, &FeatureDef)> {
        let features = self.iter_feature_defs().map(move |f| (self, f));
        let imported: Vec<_> = self
            .all_imports
            .values()
            .flat_map(|fm| fm.iter_all_feature_defs())
            .collect();
        features.chain(imported)
    }

    #[allow(unused)]
    pub(crate) fn iter_imported_files(&self) -> Vec<ImportedModule> {
        let map = &self.all_imports;

        self.imported_features
            .iter()
            .filter_map(|(id, features)| {
                let fm = map.get(id).to_owned()?;
                Some(ImportedModule::new(id.clone(), fm, features))
            })
            .collect()
    }

    pub fn find_object(&self, nm: &str) -> Option<&ObjectDef> {
        self.obj_defs.get(nm)
    }

    pub fn find_enum(&self, nm: &str) -> Option<&EnumDef> {
        self.enum_defs.get(nm)
    }

    pub fn get_feature(&self, nm: &str) -> Option<&FeatureDef> {
        self.feature_defs.get(nm)
    }

    pub fn get_coenrolling_feature_ids(&self) -> Vec<String> {
        self.iter_all_feature_defs()
            .filter(|(_, f)| f.allow_coenrollment())
            .map(|(_, f)| f.name())
            .collect()
    }

    pub fn find_feature(&self, nm: &str) -> Option<(&FeatureManifest, &FeatureDef)> {
        if let Some(f) = self.get_feature(nm) {
            Some((self, f))
        } else {
            self.all_imports.values().find_map(|fm| fm.find_feature(nm))
        }
    }

    pub fn find_import(&self, id: &ModuleId) -> Option<&FeatureManifest> {
        self.all_imports.get(id)
    }

    pub fn default_json(&self) -> Value {
        Value::Object(
            self.iter_all_feature_defs()
                .map(|(_, f)| (f.name(), f.default_json()))
                .collect(),
        )
    }

    /// This function is used to validate a new value for a feature. It accepts a feature name and
    /// a feature value, and returns a Result containing a FeatureDef.
    ///
    /// If the value is invalid for the feature, it will return an Err result.
    ///
    /// If the value is valid for the feature, it will return an Ok result with a new FeatureDef
    /// with the supplied feature value applied to the feature's property defaults.
    pub fn validate_feature_config(
        &self,
        feature_name: &str,
        feature_value: Value,
    ) -> Result<FeatureDef> {
        let (manifest, feature_def) = self
            .find_feature(feature_name)
            .ok_or_else(|| InvalidFeatureError(feature_name.to_string()))?;

        let merger = DefaultsMerger::new(&manifest.obj_defs, Default::default(), None);

        let mut feature_def = feature_def.clone();
        merger.merge_feature_defaults(&mut feature_def, &Some(vec![feature_value.into()]))?;
        manifest.validate_feature_def(&feature_def)?;
        Ok(feature_def)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct FeatureDef {
    pub(crate) name: String,
    pub(crate) doc: String,
    pub(crate) props: Vec<PropDef>,
    pub(crate) allow_coenrollment: bool,
}

impl FeatureDef {
    pub fn new(name: &str, doc: &str, props: Vec<PropDef>, allow_coenrollment: bool) -> Self {
        Self {
            name: name.into(),
            doc: doc.into(),
            props,
            allow_coenrollment,
        }
    }
    pub fn name(&self) -> String {
        self.name.clone()
    }
    pub fn doc(&self) -> String {
        self.doc.clone()
    }
    pub fn props(&self) -> Vec<PropDef> {
        self.props.clone()
    }
    pub fn allow_coenrollment(&self) -> bool {
        self.allow_coenrollment
    }

    pub fn default_json(&self) -> Value {
        let mut props = Map::new();

        for prop in self.props().iter() {
            props.insert(prop.name(), prop.default());
        }

        Value::Object(props)
    }

    pub fn has_prefs(&self) -> bool {
        self.props.iter().any(|p| p.has_prefs())
    }
}
impl TypeFinder for FeatureDef {
    fn find_types(&self, types: &mut HashSet<TypeRef>) {
        for p in self.props() {
            p.find_types(types);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct EnumDef {
    pub name: String,
    pub doc: String,
    pub variants: Vec<VariantDef>,
}

impl EnumDef {
    pub fn name(&self) -> String {
        self.name.clone()
    }
    pub fn doc(&self) -> String {
        self.doc.clone()
    }
    pub fn variants(&self) -> Vec<VariantDef> {
        self.variants.clone()
    }
}

impl TypeFinder for EnumDef {
    fn find_types(&self, types: &mut HashSet<TypeRef>) {
        types.insert(TypeRef::Enum(self.name()));
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FromStringDef {
    pub name: String,
    pub doc: String,
    pub variants: Vec<VariantDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct VariantDef {
    pub(crate) name: String,
    pub(crate) doc: String,
}
impl VariantDef {
    pub fn new(name: &str, doc: &str) -> Self {
        Self {
            name: name.into(),
            doc: doc.into(),
        }
    }
    pub fn name(&self) -> String {
        self.name.clone()
    }
    pub fn doc(&self) -> String {
        self.doc.clone()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ObjectDef {
    pub(crate) name: String,
    pub(crate) doc: String,
    pub(crate) props: Vec<PropDef>,
}

#[allow(unused)]
impl ObjectDef {
    pub fn new(name: &str, doc: &str, props: Vec<PropDef>) -> Self {
        Self {
            name: name.into(),
            doc: doc.into(),
            props,
        }
    }
    pub(crate) fn name(&self) -> String {
        self.name.clone()
    }
    pub(crate) fn doc(&self) -> String {
        self.doc.clone()
    }
    pub fn props(&self) -> Vec<PropDef> {
        self.props.clone()
    }

    pub(crate) fn find_prop(&self, nm: &str) -> PropDef {
        self.props
            .iter()
            .find(|p| p.name == nm)
            .unwrap_or_else(|| unreachable!("Can't find {}. This is a bug in FML", nm))
            .clone()
    }
}
impl TypeFinder for ObjectDef {
    fn find_types(&self, types: &mut HashSet<TypeRef>) {
        for p in self.props() {
            p.find_types(types);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PropDef {
    pub(crate) name: String,
    pub(crate) doc: String,
    #[serde(rename = "type")]
    pub(crate) typ: TypeRef,
    pub(crate) default: Literal,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) pref_key: Option<String>,
}

impl PropDef {
    pub fn name(&self) -> String {
        self.name.clone()
    }
    pub fn doc(&self) -> String {
        self.doc.clone()
    }
    pub fn typ(&self) -> TypeRef {
        self.typ.clone()
    }
    pub fn default(&self) -> Literal {
        self.default.clone()
    }
    pub fn has_prefs(&self) -> bool {
        self.pref_key.is_some() && self.typ.supports_prefs()
    }
    pub fn pref_key(&self) -> Option<String> {
        self.pref_key.clone()
    }
}

impl TypeFinder for PropDef {
    fn find_types(&self, types: &mut HashSet<TypeRef>) {
        types.insert(self.typ());
    }
}

pub type Literal = Value;

#[allow(unused)]
#[derive(Debug, Clone)]
pub(crate) struct ImportedModule<'a> {
    pub(crate) id: ModuleId,
    pub(crate) fm: &'a FeatureManifest,
    features: &'a BTreeSet<String>,
}

#[allow(unused)]
impl<'a> ImportedModule<'a> {
    pub(crate) fn new(
        id: ModuleId,
        fm: &'a FeatureManifest,
        features: &'a BTreeSet<String>,
    ) -> Self {
        Self { id, fm, features }
    }

    pub(crate) fn about(&self) -> &AboutBlock {
        &self.fm.about
    }

    pub(crate) fn features(&self) -> Vec<&'a FeatureDef> {
        let fm = self.fm;
        self.features
            .iter()
            .filter_map(|f| fm.get_feature(f))
            .collect()
    }
}

#[cfg(test)]
pub mod unit_tests {
    use serde_json::{json, Number};

    use super::*;
    use crate::error::Result;
    use crate::fixtures::intermediate_representation::get_simple_homescreen_feature;

    impl PropDef {
        pub(crate) fn new(nm: &str, typ: TypeRef, default: Value) -> Self {
            PropDef {
                name: nm.into(),
                doc: format!("{nm} property of type {typ}"),
                typ,
                default,
                pref_key: None,
            }
        }

        pub(crate) fn new_with_doc(nm: &str, doc: &str, typ: TypeRef, default: Value) -> Self {
            PropDef {
                name: nm.into(),
                doc: doc.into(),
                typ,
                default,
                pref_key: None,
            }
        }
    }

    #[test]
    fn can_ir_represent_smoke_test() -> Result<()> {
        let reference_manifest = get_simple_homescreen_feature();
        let json_string = serde_json::to_string(&reference_manifest)?;
        let manifest_from_json: FeatureManifest = serde_json::from_str(&json_string)?;

        assert_eq!(reference_manifest, manifest_from_json);

        Ok(())
    }

    #[test]
    fn validate_good_feature_manifest() -> Result<()> {
        let fm = get_simple_homescreen_feature();
        fm.validate_manifest()
    }

    #[test]
    fn validate_duplicate_enum_defs_fail() -> Result<()> {
        let mut fm = get_simple_homescreen_feature();
        fm.enum_defs.insert(
            "HomeScreenSection".into(),
            EnumDef {
                name: "HomeScreenSection".into(),
                doc: "The sections of the homescreen".into(),
                variants: vec![
                    VariantDef::new("top-sites", "The original frecency sorted sites"),
                    VariantDef::new("jump-back-in", "Jump back in section"),
                    VariantDef::new("recently-saved", "Tabs that have been bookmarked recently"),
                ],
            },
        );
        fm.validate_manifest()
            .expect_err("Should fail on duplicate enum_defs");
        Ok(())
    }

    #[test]
    fn validate_allow_coenrollment() -> Result<()> {
        let mut fm = get_simple_homescreen_feature();
        fm.add_feature(FeatureDef::new(
            "some_def",
            "my lovely qtest doc",
            vec![PropDef::new("some prop", TypeRef::String, json!("default"))],
            true,
        ));
        fm.validate_manifest()?;
        let coenrolling_ids = fm.get_coenrolling_feature_ids();
        assert_eq!(coenrolling_ids, vec!["some_def".to_string()]);

        Ok(())
    }

    #[test]
    fn validate_duplicate_feature_defs_fails() -> Result<()> {
        let mut fm = get_simple_homescreen_feature();
        fm.add_feature(FeatureDef::new(
            "homescreen",
            "Represents the homescreen feature",
            vec![PropDef::new(
                "sections-enabled",
                TypeRef::EnumMap(
                    Box::new(TypeRef::Enum("SectionId".into())),
                    Box::new(TypeRef::String),
                ),
                json!({
                    "top-sites": true,
                    "jump-back-in": false,
                    "recently-saved": false,
                }),
            )],
            false,
        ));
        fm.validate_manifest()
            .expect_err("Should fail on duplicate feature defs");
        Ok(())
    }

    #[test]
    fn validate_duplicate_props_in_same_feature_fails() -> Result<()> {
        let mut fm = get_simple_homescreen_feature();
        fm.add_feature(FeatureDef::new(
            "otherhomescreen",
            "Represents the homescreen feature",
            vec![
                PropDef::new(
                    "duplicate-prop",
                    TypeRef::EnumMap(
                        Box::new(TypeRef::Enum("SectionId".into())),
                        Box::new(TypeRef::String),
                    ),
                    json!({
                        "top-sites": true,
                        "jump-back-in": false,
                        "recently-saved": false,
                    }),
                ),
                PropDef::new(
                    "duplicate-prop",
                    TypeRef::EnumMap(
                        Box::new(TypeRef::Enum("SectionId".into())),
                        Box::new(TypeRef::String),
                    ),
                    json!({
                        "top-sites": true,
                        "jump-back-in": false,
                        "recently-saved": false,
                    }),
                ),
            ],
            false,
        ));
        fm.validate_manifest()
            .expect_err("Should fail on duplicate props in the same feature");
        Ok(())
    }

    #[test]
    fn validate_enum_type_ref_doesnt_match_def() -> Result<()> {
        let mut fm = get_simple_homescreen_feature();
        fm.add_feature(FeatureDef::new(
            "some_def",
            "test doc",
            vec![PropDef::new(
                "prop name",
                TypeRef::Enum("EnumDoesntExist".into()),
                json!(null),
            )],
            false,
        ));
        fm.validate_manifest().expect_err(
            "Should fail since EnumDoesntExist isn't a an enum defined in the manifest",
        );
        Ok(())
    }

    #[test]
    fn validate_obj_type_ref_doesnt_match_def() -> Result<()> {
        let mut fm = get_simple_homescreen_feature();
        fm.add_feature(FeatureDef::new(
            "some_def",
            "test doc",
            vec![PropDef::new(
                "prop name",
                TypeRef::Object("ObjDoesntExist".into()),
                json!(null),
            )],
            false,
        ));
        fm.validate_manifest().expect_err(
            "Should fail since ObjDoesntExist isn't a an Object defined in the manifest",
        );
        Ok(())
    }

    #[test]
    fn validate_enum_map_with_non_enum_key() -> Result<()> {
        let mut fm = get_simple_homescreen_feature();
        fm.add_feature(FeatureDef::new(
            "some_def",
            "test doc",
            vec![PropDef::new(
                "prop name",
                TypeRef::EnumMap(Box::new(TypeRef::String), Box::new(TypeRef::String)),
                json!(null),
            )],
            false,
        ));
        fm.validate_manifest()
            .expect_err("Should fail since the key on an EnumMap must be an Enum");
        Ok(())
    }

    #[test]
    fn validate_list_with_enum_with_no_def() -> Result<()> {
        let mut fm = get_simple_homescreen_feature();
        fm.add_feature(FeatureDef::new(
            "some_def",
            "test doc",
            vec![PropDef::new(
                "prop name",
                TypeRef::List(Box::new(TypeRef::Enum("EnumDoesntExist".into()))),
                json!(null),
            )],
            false,
        ));
        fm.validate_manifest()
            .expect_err("Should fail EnumDoesntExist isn't a an enum defined in the manifest");
        Ok(())
    }

    #[test]
    fn validate_enum_map_with_enum_with_no_def() -> Result<()> {
        let mut fm = get_simple_homescreen_feature();
        fm.add_feature(FeatureDef::new(
            "some_def",
            "test doc",
            vec![PropDef::new(
                "prop name",
                TypeRef::EnumMap(
                    Box::new(TypeRef::Enum("EnumDoesntExist".into())),
                    Box::new(TypeRef::String),
                ),
                json!(null),
            )],
            false,
        ));
        fm.validate_manifest().expect_err(
            "Should fail since EnumDoesntExist isn't a an enum defined in the manifest",
        );
        Ok(())
    }

    #[test]
    fn validate_enum_map_with_obj_value_no_def() -> Result<()> {
        let mut fm = get_simple_homescreen_feature();
        fm.add_feature(FeatureDef::new(
            "some_def",
            "test doc",
            vec![PropDef::new(
                "prop name",
                TypeRef::EnumMap(
                    Box::new(TypeRef::Enum("SectionId".into())),
                    Box::new(TypeRef::Object("ObjDoesntExist".into())),
                ),
                json!(null),
            )],
            false,
        ));
        fm.validate_manifest()
            .expect_err("Should fail since ObjDoesntExist isn't an Object defined in the manifest");
        Ok(())
    }

    #[test]
    fn validate_string_map_with_enum_value_no_def() -> Result<()> {
        let mut fm = get_simple_homescreen_feature();
        fm.add_feature(FeatureDef::new(
            "some_def",
            "test doc",
            vec![PropDef::new(
                "prop name",
                TypeRef::StringMap(Box::new(TypeRef::Enum("EnumDoesntExist".into()))),
                json!(null),
            )],
            false,
        ));
        fm.validate_manifest()
            .expect_err("Should fail since ObjDoesntExist isn't an Object defined in the manifest");
        Ok(())
    }

    #[test]
    fn validate_nested_optionals_fail() -> Result<()> {
        let mut fm = get_simple_homescreen_feature();
        fm.add_feature(FeatureDef::new(
            "some_def",
            "test doc",
            vec![PropDef::new(
                "prop name",
                TypeRef::Option(Box::new(TypeRef::Option(Box::new(TypeRef::String)))),
                json!(null),
            )],
            false,
        ));
        fm.validate_manifest()
            .expect_err("Should fail since we can't have nested optionals");
        Ok(())
    }

    pub fn get_feature_manifest(
        obj_defs: Vec<ObjectDef>,
        enum_defs: Vec<EnumDef>,
        feature_defs: Vec<FeatureDef>,
        all_imports: HashMap<ModuleId, FeatureManifest>,
    ) -> FeatureManifest {
        FeatureManifest {
            enum_defs: map_from(enum_defs, |e| e.name()),
            obj_defs: map_from(obj_defs, |o| o.name()),
            feature_defs: map_from(feature_defs, |f| f.name()),
            all_imports,
            ..Default::default()
        }
    }

    fn get_one_prop_feature_manifest(
        obj_defs: Vec<ObjectDef>,
        enum_defs: Vec<EnumDef>,
        prop: &PropDef,
    ) -> FeatureManifest {
        FeatureManifest {
            enum_defs: map_from(enum_defs, |e| e.name()),
            obj_defs: map_from(obj_defs, |o| o.name()),
            feature_defs: BTreeMap::from([(
                "".to_string(),
                FeatureDef {
                    props: vec![prop.clone()],
                    ..Default::default()
                },
            )]),
            ..Default::default()
        }
    }

    fn get_one_prop_feature_manifest_with_imports(
        obj_defs: Vec<ObjectDef>,
        enum_defs: Vec<EnumDef>,
        prop: &PropDef,
        all_imports: HashMap<ModuleId, FeatureManifest>,
    ) -> FeatureManifest {
        let mut fm = FeatureManifest {
            enum_defs: map_from(enum_defs, |e| e.name()),
            obj_defs: map_from(obj_defs, |o| o.name()),
            all_imports,
            ..Default::default()
        };
        fm.add_feature(FeatureDef {
            props: vec![prop.clone()],
            ..Default::default()
        });
        fm
    }

    fn map_from<T, F, K>(list: Vec<T>, key: F) -> BTreeMap<K, T>
    where
        K: Ord,
        F: Fn(&T) -> K,
    {
        let mut res: BTreeMap<K, T> = Default::default();

        for t in list {
            let k = key(&t);
            res.insert(k, t);
        }

        res
    }

    #[test]
    fn test_validate_prop_defaults_string() -> Result<()> {
        let mut prop = PropDef::new("key", TypeRef::String, json!("default!"));
        let fm = get_one_prop_feature_manifest(vec![], vec![], &prop);
        let path = format!("test_validate_prop_defaults_string.{}", prop.name);
        fm.validate_prop_defaults(&path, &prop)?;
        prop.default = json!(100);
        let fm = get_one_prop_feature_manifest(vec![], vec![], &prop);
        fm.validate_prop_defaults(&path, &prop)
            .expect_err("Should error out, default is number when it should be string");
        Ok(())
    }

    #[test]
    fn test_validate_prop_defaults_int() -> Result<()> {
        let mut prop = PropDef::new("key", TypeRef::Int, json!(100));
        let fm = get_one_prop_feature_manifest(vec![], vec![], &prop);
        let path = format!("test_validate_prop_defaults_int.{}", prop.name);
        fm.validate_prop_defaults(&path, &prop)?;
        prop.default = json!("100");
        let fm = get_one_prop_feature_manifest(vec![], vec![], &prop);
        fm.validate_prop_defaults(&path, &prop)
            .expect_err("Should error out, default is string when it should be number");
        Ok(())
    }

    #[test]
    fn test_validate_prop_defaults_bool() -> Result<()> {
        let mut prop = PropDef::new("key", TypeRef::Boolean, json!(true));
        let fm = get_one_prop_feature_manifest(vec![], vec![], &prop);
        let path = format!("test_validate_prop_defaults_bool.{}", prop.name);
        fm.validate_prop_defaults(&path, &prop)?;
        prop.default = json!("100");
        let fm = get_one_prop_feature_manifest(vec![], vec![], &prop);
        fm.validate_prop_defaults(&path, &prop)
            .expect_err("Should error out, default is string when it should be a boolean");
        Ok(())
    }

    #[test]
    fn test_validate_prop_defaults_bundle_image() -> Result<()> {
        let mut prop = PropDef::new(
            "key",
            TypeRef::BundleImage("Icon".into()),
            json!("IconBlue"),
        );
        let fm = get_one_prop_feature_manifest(vec![], vec![], &prop);
        let path = format!("test_validate_prop_defaults_bundle_image.{}", prop.name);
        fm.validate_prop_defaults(&path, &prop)?;
        prop.default = json!(100);
        let fm = get_one_prop_feature_manifest(vec![], vec![], &prop);
        fm.validate_prop_defaults(&path, &prop).expect_err(
            "Should error out, default is number when it should be a string (bundleImage string)",
        );
        Ok(())
    }

    #[test]
    fn test_validate_prop_defaults_bundle_text() -> Result<()> {
        let mut prop = PropDef::new(
            "key",
            TypeRef::BundleText("Text".into()),
            json!("BundledText"),
        );
        let fm = get_one_prop_feature_manifest(vec![], vec![], &prop);
        let path = format!("test_validate_prop_defaults_bundle_text.{}", prop.name);
        fm.validate_prop_defaults(&path, &prop)?;
        prop.default = json!(100);
        let fm = get_one_prop_feature_manifest(vec![], vec![], &prop);
        fm.validate_prop_defaults(&path, &prop).expect_err(
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
        let fm = get_one_prop_feature_manifest(vec![], vec![], &prop);
        let path = format!("test_validate_prop_defaults_option_null.{}", prop.name);
        fm.validate_prop_defaults(&path, &prop)?;
        prop.default = json!(100);
        let fm = get_one_prop_feature_manifest(vec![], vec![], &prop);
        fm.validate_prop_defaults(&path, &prop).expect_err(
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
        let fm = get_one_prop_feature_manifest(vec![], vec![], &prop);
        let path = format!("test_validate_prop_defaults_nested_options.{}", prop.name);
        fm.validate_prop_defaults(&path, &prop)
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
        let fm = get_one_prop_feature_manifest(vec![], vec![], &prop);
        let path = format!("test_validate_prop_defaults_option_non_null.{}", prop.name);
        fm.validate_prop_defaults(&path, &prop)?;
        prop.default = json!(100);
        let fm = get_one_prop_feature_manifest(vec![], vec![], &prop);
        fm.validate_prop_defaults(&path, &prop).expect_err(
            "Should error out, default is number when it should be a boolean (Optional boolean)",
        );
        Ok(())
    }

    #[test]
    fn test_validate_prop_defaults_enum() -> Result<()> {
        let mut prop = PropDef::new("key", TypeRef::Enum("ButtonColor".into()), json!("blue"));
        let enum_defs = vec![EnumDef {
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
        }];
        let fm = get_one_prop_feature_manifest(vec![], enum_defs.clone(), &prop);
        let path = format!("test_validate_prop_defaults_enum.{}", prop.name);
        fm.validate_prop_defaults(&path, &prop)?;
        prop.default = json!("green");
        let fm = get_one_prop_feature_manifest(vec![], enum_defs.clone(), &prop);
        fm.validate_prop_defaults(&path, &prop)?;
        prop.default = json!("not a valid color");
        let fm = get_one_prop_feature_manifest(vec![], enum_defs.clone(), &prop);
        fm.validate_prop_defaults(&path, &prop)
            .expect_err("Should error out since default is not a valid enum variant");
        prop.default = json!("blue");
        prop.typ = TypeRef::Enum("DoesntExist".into());
        let fm = get_one_prop_feature_manifest(vec![], enum_defs, &prop);
        fm.validate_prop_defaults(&path, &prop)
            .expect_err("Should error out since the enum definition doesn't exist for the TypeRef");
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
        let enum_defs = vec![EnumDef {
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
        }];
        let fm = get_one_prop_feature_manifest(vec![], enum_defs.clone(), &prop);
        let path = format!("test_validate_prop_defaults_enum_map.{}", prop.name);
        fm.validate_prop_defaults(&path, &prop)?;
        prop.default = json!({
            "blue": 1,
        });
        let fm = get_one_prop_feature_manifest(vec![], enum_defs.clone(), &prop);
        fm.validate_prop_defaults(&path, &prop)
            .expect_err("Should error out because the enum map is missing the green key");
        prop.default = json!({
            "blue": 1,
            "green": 22,
            "red": 3,
        });
        let fm = get_one_prop_feature_manifest(vec![], enum_defs, &prop);
        fm.validate_prop_defaults(&path, &prop).expect_err("Should error out because the default includes an extra key that is not a variant of the enum (red)");
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
        let path = format!("test_validate_prop_defaults_string_map.{}", &prop.name);
        let fm = get_one_prop_feature_manifest(vec![], vec![], &prop);
        fm.validate_prop_defaults(&path, &prop)?;
        prop.default = json!({
            "blue": 1,
        });
        let fm = get_one_prop_feature_manifest(vec![], vec![], &prop);
        fm.validate_prop_defaults(&path, &prop)?;
        prop.default = json!({
            "blue": 1,
            "green": 22,
            "red": 3,
            "white": "AHA not a number"
        });
        let fm = get_one_prop_feature_manifest(vec![], vec![], &prop);
        fm.validate_prop_defaults(&path, &prop).expect_err("Should error out because the string map includes a value that is not an int as defined by the TypeRef");
        Ok(())
    }

    #[test]
    fn test_validate_prop_defaults_list() -> Result<()> {
        let mut prop = PropDef::new(
            "key",
            TypeRef::List(Box::new(TypeRef::Int)),
            json!([1, 3, 100]),
        );
        let path = format!("test_validate_prop_defaults_list.{}", &prop.name);
        let fm = get_one_prop_feature_manifest(vec![], vec![], &prop);
        fm.validate_prop_defaults(&path, &prop)?;
        prop.default = json!([1, 2, "oops"]);
        let fm = get_one_prop_feature_manifest(vec![], vec![], &prop);
        fm.validate_prop_defaults(&path, &prop)
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
        let obj_defs = vec![
            ObjectDef {
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
            },
            ObjectDef {
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
            },
        ];
        let enum_defs = vec![EnumDef {
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
        }];
        let path = format!("test_validate_prop_defaults_object.{}", &prop.name);
        let fm = get_one_prop_feature_manifest(obj_defs.clone(), enum_defs.clone(), &prop);
        fm.validate_prop_defaults(&path, &prop)?;
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
        let fm = get_one_prop_feature_manifest(obj_defs.clone(), enum_defs.clone(), &prop);
        fm.validate_prop_defaults(&path, &prop).expect_err(
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
        let fm = get_one_prop_feature_manifest(obj_defs.clone(), enum_defs.clone(), &prop);
        fm.validate_prop_defaults(&path, &prop)
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
        let fm = get_one_prop_feature_manifest(obj_defs.clone(), enum_defs.clone(), &prop);

        fm.validate_prop_defaults(&path, &prop)?;

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
        let fm = get_one_prop_feature_manifest(obj_defs, enum_defs, &prop);
        // OK, because we are missing `optional` which is optional anyways
        fm.validate_prop_defaults(&path, &prop)?;
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
        let enum_defs = vec![EnumDef {
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
        }];
        let fm = get_one_prop_feature_manifest(vec![], enum_defs, &prop);
        // OK because the value is optional, and thus it's okay if it's missing (green is missing from the default)
        let path = format!("test.{}", &prop.name);
        fm.validate_prop_defaults(&path, &prop)?;
        Ok(())
    }

    #[test]
    fn test_iter_object_defs_deep_iterates_on_all_imports() -> Result<()> {
        let prop_i = PropDef::new(
            "key_i",
            TypeRef::Object("SampleObjImported".into()),
            json!({
                "string": "bobo",
            }),
        );
        let obj_defs_i = vec![ObjectDef {
            name: "SampleObjImported".into(),
            props: vec![PropDef::new("string", TypeRef::String, json!("a string"))],
            ..Default::default()
        }];
        let fm_i = get_one_prop_feature_manifest(obj_defs_i, vec![], &prop_i);

        let prop = PropDef::new(
            "key",
            TypeRef::Object("SampleObj".into()),
            json!({
                "string": "bobo",
            }),
        );
        let obj_defs = vec![ObjectDef {
            name: "SampleObj".into(),
            props: vec![PropDef::new("string", TypeRef::String, json!("a string"))],
            ..Default::default()
        }];
        let fm = get_one_prop_feature_manifest_with_imports(
            obj_defs,
            vec![],
            &prop,
            HashMap::from([(ModuleId::Local("test".into()), fm_i)]),
        );

        let names: Vec<String> = fm.iter_all_object_defs().map(|(_, o)| o.name()).collect();

        assert_eq!(names[0], "SampleObj".to_string());
        assert_eq!(names[1], "SampleObjImported".to_string());

        Ok(())
    }

    #[test]
    fn test_iter_feature_defs_deep_iterates_on_all_imports() -> Result<()> {
        let prop_i = PropDef::new("key_i", TypeRef::String, json!("string"));
        let fm_i = get_one_prop_feature_manifest(vec![], vec![], &prop_i);

        let prop = PropDef::new("key", TypeRef::String, json!("string"));
        let fm = get_one_prop_feature_manifest_with_imports(
            vec![],
            vec![],
            &prop,
            HashMap::from([(ModuleId::Local("test".into()), fm_i)]),
        );

        let names: Vec<String> = fm
            .iter_all_feature_defs()
            .map(|(_, f)| f.props[0].name())
            .collect();

        assert_eq!(names[0], "key".to_string());
        assert_eq!(names[1], "key_i".to_string());

        Ok(())
    }

    #[test]
    fn test_find_feature_deep_finds_across_all_imports() -> Result<()> {
        let fm_i = get_feature_manifest(
            vec![],
            vec![],
            vec![FeatureDef {
                name: "feature_i".into(),
                ..Default::default()
            }],
            HashMap::new(),
        );

        let fm = get_feature_manifest(
            vec![],
            vec![],
            vec![FeatureDef {
                name: "feature".into(),
                ..Default::default()
            }],
            HashMap::from([(ModuleId::Local("test".into()), fm_i)]),
        );

        let feature = fm.find_feature("feature_i");

        assert!(feature.is_some());

        Ok(())
    }

    #[test]
    fn test_get_coenrolling_feature_finds_across_all_imports() -> Result<()> {
        let fm_i = get_feature_manifest(
            vec![],
            vec![],
            vec![
                FeatureDef {
                    name: "coenrolling_import_1".into(),
                    allow_coenrollment: true,
                    ..Default::default()
                },
                FeatureDef {
                    name: "coenrolling_import_2".into(),
                    allow_coenrollment: true,
                    ..Default::default()
                },
            ],
            HashMap::new(),
        );

        let fm = get_feature_manifest(
            vec![],
            vec![],
            vec![
                FeatureDef {
                    name: "coenrolling_feature".into(),
                    allow_coenrollment: true,
                    ..Default::default()
                },
                FeatureDef {
                    name: "non_coenrolling_feature".into(),
                    allow_coenrollment: false,
                    ..Default::default()
                },
            ],
            HashMap::from([(ModuleId::Local("test".into()), fm_i)]),
        );

        let coenrolling_features = fm.get_coenrolling_feature_ids();
        let expected = vec![
            "coenrolling_feature".to_string(),
            "coenrolling_import_1".to_string(),
            "coenrolling_import_2".to_string(),
        ];

        assert_eq!(coenrolling_features, expected);

        Ok(())
    }

    #[test]
    fn test_no_coenrolling_feature_finds_across_all_imports() -> Result<()> {
        let fm_i = get_feature_manifest(
            vec![],
            vec![],
            vec![FeatureDef {
                name: "not_coenrolling_import".into(),
                allow_coenrollment: false,
                ..Default::default()
            }],
            HashMap::new(),
        );

        let fm = get_feature_manifest(
            vec![],
            vec![],
            vec![
                FeatureDef {
                    name: "non_coenrolling_feature_1".into(),
                    allow_coenrollment: false,
                    ..Default::default()
                },
                FeatureDef {
                    name: "non_coenrolling_feature_2".into(),
                    allow_coenrollment: false,
                    ..Default::default()
                },
            ],
            HashMap::from([(ModuleId::Local("test".into()), fm_i)]),
        );

        let coenrolling_features = fm.get_coenrolling_feature_ids();
        let expected: Vec<String> = vec![];

        assert_eq!(coenrolling_features, expected);

        Ok(())
    }

    #[test]
    fn test_default_json_works_across_all_imports() -> Result<()> {
        let fm_i = get_feature_manifest(
            vec![],
            vec![],
            vec![FeatureDef {
                name: "feature_i".into(),
                props: vec![PropDef::new(
                    "prop_i_1",
                    TypeRef::String,
                    Value::String("prop_i_1_value".into()),
                )],
                ..Default::default()
            }],
            HashMap::new(),
        );

        let fm = get_feature_manifest(
            vec![],
            vec![],
            vec![FeatureDef {
                name: "feature".into(),
                props: vec![PropDef::new(
                    "prop_1",
                    TypeRef::String,
                    Value::String("prop_1_value".into()),
                )],
                ..Default::default()
            }],
            HashMap::from([(ModuleId::Local("test".into()), fm_i)]),
        );

        let json = fm.default_json();
        assert_eq!(
            json.get("feature_i").unwrap().get("prop_i_1").unwrap(),
            &Value::String("prop_i_1_value".into())
        );
        assert_eq!(
            json.get("feature").unwrap().get("prop_1").unwrap(),
            &Value::String("prop_1_value".into())
        );

        Ok(())
    }

    #[test]
    fn test_validate_feature_config_success() -> Result<()> {
        let fm = get_feature_manifest(
            vec![],
            vec![],
            vec![FeatureDef {
                name: "feature".into(),
                props: vec![PropDef::new(
                    "prop_1",
                    TypeRef::String,
                    Value::String("prop_1_value".into()),
                )],
                ..Default::default()
            }],
            HashMap::new(),
        );

        let result = fm.validate_feature_config(
            "feature",
            Value::Object(Map::from_iter([(
                "prop_1".to_string(),
                Value::String("new value".into()),
            )])),
        )?;
        assert_eq!(result.props[0].default, Value::String("new value".into()));

        Ok(())
    }

    #[test]
    fn test_validate_feature_config_invalid_feature_name() -> Result<()> {
        let fm = get_feature_manifest(
            vec![],
            vec![],
            vec![FeatureDef {
                name: "feature".into(),
                props: vec![PropDef::new(
                    "prop_1",
                    TypeRef::String,
                    Value::String("prop_1_value".into()),
                )],
                ..Default::default()
            }],
            HashMap::new(),
        );

        let result = fm.validate_feature_config(
            "feature-1",
            Value::Object(Map::from_iter([(
                "prop_1".to_string(),
                Value::String("new value".into()),
            )])),
        );
        assert!(result.is_err());
        assert_eq!(
            result.err().unwrap().to_string(),
            "Feature `feature-1` not found on manifest".to_string()
        );

        Ok(())
    }

    #[test]
    fn test_validate_feature_config_invalid_feature_prop_name() -> Result<()> {
        let fm = get_feature_manifest(
            vec![],
            vec![],
            vec![FeatureDef {
                name: "feature".into(),
                props: vec![PropDef::new(
                    "prop_1",
                    TypeRef::Option(Box::new(TypeRef::String)),
                    Value::Null,
                )],
                ..Default::default()
            }],
            HashMap::new(),
        );

        let result = fm.validate_feature_config(
            "feature",
            Value::Object(Map::from_iter([(
                "prop".to_string(),
                Value::String("new value".into()),
            )])),
        );
        assert!(result.is_err());
        assert_eq!(
            result.err().unwrap().to_string(),
            "Validation Error at features/feature: Invalid property \"prop\"; did you mean \"prop_1\"?"
        );

        Ok(())
    }

    #[test]
    fn test_validate_feature_config_invalid_feature_prop_value() -> Result<()> {
        let fm = get_feature_manifest(
            vec![],
            vec![],
            vec![FeatureDef {
                name: "feature".into(),
                props: vec![PropDef::new(
                    "prop_1",
                    TypeRef::String,
                    json!("prop_1_value"),
                )],
                ..Default::default()
            }],
            HashMap::new(),
        );

        let result = fm.validate_feature_config(
            "feature",
            Value::Object(Map::from_iter([(
                "prop_1".to_string(),
                Value::Number(Number::from(1)),
            )])),
        );
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().to_string(), "Validation Error at features/feature.prop_1: Mismatch between type String and default 1".to_string());

        Ok(())
    }

    #[test]
    fn test_validate_feature_config_errors_on_invalid_object_prop() -> Result<()> {
        let obj_defs = vec![ObjectDef {
            name: "SampleObj".into(),
            props: vec![PropDef::new("string", TypeRef::String, json!("a string"))],
            ..Default::default()
        }];
        let fm = get_feature_manifest(
            obj_defs,
            vec![],
            vec![FeatureDef {
                name: "feature".into(),
                props: vec![PropDef::new(
                    "prop_1",
                    TypeRef::Object("SampleObj".into()),
                    json!({
                        "string": "a value"
                    }),
                )],
                ..Default::default()
            }],
            HashMap::new(),
        );

        let result = fm.validate_feature_config(
            "feature",
            json!({
                "prop_1": {
                    "invalid-prop": "invalid-prop value"
                }
            }),
        );

        assert!(result.is_err());
        assert_eq!(
            result.err().unwrap().to_string(),
            "Validation Error at features/feature.prop_1#SampleObj: Invalid key \"invalid-prop\" for object SampleObj; did you mean \"string\"?"
        );

        Ok(())
    }
}
