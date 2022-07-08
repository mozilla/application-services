/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    commands::TargetLanguage,
    error::{FMLError, Result},
    intermediate_representation::{
        EnumDef, FeatureDef, FeatureManifest, ModuleId, ObjectDef, PropDef, TypeRef, VariantDef,
    },
    util::loaders::{FileLoader, FilePath},
};

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub(crate) struct EnumVariantBody {
    description: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub(crate) struct EnumBody {
    description: String,
    variants: HashMap<String, EnumVariantBody>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub(crate) struct FieldBody {
    description: String,
    #[serde(default)]
    required: bool,
    #[serde(rename = "type")]
    variable_type: String,
    default: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub(crate) struct ObjectBody {
    description: String,
    failable: Option<bool>,
    // We need these in a deterministic order, so they are stable across multiple
    // runs of the same manifests.
    fields: BTreeMap<String, FieldBody>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct Types {
    #[serde(default)]
    enums: HashMap<String, EnumBody>,
    #[serde(default)]
    objects: HashMap<String, ObjectBody>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct AboutBlock {
    pub(crate) description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "kotlin", alias = "android")]
    pub(crate) kotlin_about: Option<KotlinAboutBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "swift", alias = "ios")]
    pub(crate) swift_about: Option<SwiftAboutBlock>,
}

impl AboutBlock {
    pub(crate) fn is_includable(&self) -> bool {
        self.kotlin_about.is_none() && self.swift_about.is_none()
    }

    pub(crate) fn supports(&self, lang: &TargetLanguage) -> bool {
        match lang {
            TargetLanguage::Kotlin => self.kotlin_about.is_some(),
            TargetLanguage::Swift => self.swift_about.is_some(),
            TargetLanguage::IR => true,
            TargetLanguage::ExperimenterYAML => true,
            TargetLanguage::ExperimenterJSON => true,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct SwiftAboutBlock {
    pub(crate) module: String,
    pub(crate) class: String,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct KotlinAboutBlock {
    pub(crate) package: String,
    pub(crate) class: String,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub(crate) struct ImportBlock {
    pub(crate) path: String,
    pub(crate) channel: String,
    #[serde(default)]
    pub(crate) features: HashMap<String, Vec<DefaultBlock>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub(crate) struct FeatureBody {
    description: String,
    // We need these in a deterministic order, so they are stable across multiple
    // runs of the same manifests:
    // 1. Swift insists on args in the same order they were declared.
    // 2. imported features are declared and constructed in different runs of the tool.
    variables: BTreeMap<String, FieldBody>,
    #[serde(alias = "defaults")]
    default: Option<Vec<DefaultBlock>>,
}
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct ManifestFrontEnd {
    #[serde(default)]
    version: String,
    #[serde(default)]
    about: Option<AboutBlock>,

    // We'd like to get rid of the `types` property,
    // but we need to keep supporting it.
    #[serde(default)]
    #[serde(rename = "types")]
    legacy_types: Option<Types>,
    #[serde(default)]
    features: HashMap<String, FeatureBody>,

    #[serde(default)]
    #[serde(alias = "include")]
    includes: Vec<String>,

    #[serde(default)]
    #[serde(alias = "import")]
    imports: Vec<ImportBlock>,

    #[serde(default)]
    channels: Vec<String>,

    // If a types attribute isn't explicitly expressed,
    // then we should assume that we use the flattened version.
    #[serde(default)]
    #[serde(flatten)]
    types: Types,
}

impl ManifestFrontEnd {
    /// Retrieves all the types represented in the Manifest
    ///
    /// # Returns
    /// Returns a [`std::collections::HashMap<String,TypeRef>`] where
    /// the key is the name of the type, and the TypeRef represents the type itself
    fn get_types(&self) -> HashMap<String, TypeRef> {
        let types = self.legacy_types.as_ref().unwrap_or(&self.types);
        types
            .enums
            .iter()
            .map(|(s, _)| (s.clone(), TypeRef::Enum(s.clone())))
            .chain(
                types
                    .objects
                    .iter()
                    .map(|(s, _)| (s.clone(), TypeRef::Object(s.clone()))),
            )
            .collect()
    }

    /// Transforms a front-end field definition, a tuple of [`String`] and [`FieldBody`],
    /// into a [`PropDef`]
    ///
    /// # Arguments
    /// - `field`: The [`(&String, &FieldBody)`] tuple to get the propdef from
    ///
    /// # Returns
    /// return the IR [`PropDef`]
    fn get_prop_def_from_field(&self, field: (&String, &FieldBody)) -> PropDef {
        let types = self.get_types();
        PropDef {
            name: field.0.into(),
            doc: field.1.description.clone(),
            typ: match get_typeref_from_string(
                field.1.variable_type.to_owned(),
                Some(types.clone()),
            ) {
                Ok(type_ref) => type_ref,
                Err(e) => {
                    // Try matching against the user defined types
                    match types.get(&field.1.variable_type) {
                        Some(type_ref) => type_ref.to_owned(),
                        None => panic!(
                            "{}\n{} is not a valid FML type or user defined type",
                            e, field.1.variable_type
                        ),
                    }
                }
            },
            default: json!(field.1.default),
        }
    }

    /// Retrieves all the feature definitions represented in the manifest
    ///
    /// # Returns
    /// Returns a [`std::vec::Vec<FeatureDef>`]
    fn get_feature_defs(&self, merger: &DefaultsMerger) -> Result<Vec<FeatureDef>> {
        self.features
            .iter()
            .map(|(name, body)| {
                let mut def = FeatureDef {
                    name: name.clone(),
                    doc: body.description.clone(),
                    props: body
                        .variables
                        .iter()
                        .map(|v| self.get_prop_def_from_field(v))
                        .collect(),
                    default: Default::default(),
                };

                merger.merge_feature_defaults(&mut def, &body.default)?;
                Ok(def)
            })
            .into_iter()
            .collect()
    }

    /// Retrieves all the Object type definitions represented in the manifest
    ///
    /// # Returns
    /// Returns a [`std::vec::Vec<ObjectDef>`]
    fn get_objects(&self) -> Vec<ObjectDef> {
        let types = self.legacy_types.as_ref().unwrap_or(&self.types);
        types
            .objects
            .iter()
            .map(|t| ObjectDef {
                name: t.0.clone(),
                doc: t.1.description.clone(),
                props: t
                    .1
                    .fields
                    .iter()
                    .map(|v| self.get_prop_def_from_field(v))
                    .collect(),
            })
            .collect()
    }

    /// Retrieves all the Enum type definitions represented in the manifest
    ///
    /// # Returns
    /// Returns a [`std::vec::Vec<EnumDef>`]
    fn get_enums(&self) -> Vec<EnumDef> {
        let types = self.legacy_types.as_ref().unwrap_or(&self.types);
        types
            .enums
            .clone()
            .into_iter()
            .map(|t| EnumDef {
                name: t.0,
                doc: t.1.description,
                variants: t
                    .1
                    .variants
                    .iter()
                    .map(|v| VariantDef {
                        name: v.0.clone(),
                        doc: v.1.description.clone(),
                    })
                    .collect(),
            })
            .collect()
    }

    fn get_intermediate_representation(
        &self,
        id: &ModuleId,
        channel: &str,
    ) -> Result<FeatureManifest> {
        let enums = self.get_enums();
        let objects = self.get_objects();

        let object_map: HashMap<String, &ObjectDef> =
            objects.iter().map(|o| (o.name(), o)).collect();
        let merger = DefaultsMerger::new(object_map, self.channels.clone(), channel.to_owned());

        let features = self.get_feature_defs(&merger)?;

        let about = match &self.about {
            Some(a) => a.clone(),
            None => Default::default(),
        };

        Ok(FeatureManifest {
            id: id.clone(),
            about,
            enum_defs: enums,
            obj_defs: objects,
            hints: HashMap::new(),
            feature_defs: features,

            ..Default::default()
        })
    }
}

fn parse_typeref_string(input: String) -> Result<(String, Option<String>)> {
    // Split the string into the TypeRef and the name
    let mut object_type_iter = input.split(&['<', '>'][..]);

    // This should be the TypeRef type (except for )
    let type_ref_name = object_type_iter.next().unwrap().trim();

    if ["String", "Int", "Boolean"].contains(&type_ref_name) {
        return Ok((type_ref_name.to_string(), None));
    }

    // This should be the name or type of the Object
    match object_type_iter.next() {
        Some(object_type_name) => Ok((
            type_ref_name.to_string(),
            Some(object_type_name.to_string()),
        )),
        None => Ok((type_ref_name.to_string(), None)),
    }
}

/// Collects the channel defaults of the feature manifest
/// and merges them by channel
///
/// **NOTE**: defaults with no channel apply to **all** channels
///
/// # Arguments
/// - `defaults`: a [`serde_json::Value`] representing the array of defaults
///
/// # Returns
/// Returns a [`std::collections::HashMap<String, serde_json::Value>`] representing
/// the merged defaults. The key is the name of the channel and the value is the
/// merged json.
///
/// # Errors
/// Will return errors in the following cases (not exhaustive):
/// - The `defaults` argument is not an array
/// - There is a `channel` in the `defaults` argument that doesn't
///     exist in the `channels` argument
fn collect_channel_defaults(
    defaults: &[DefaultBlock],
    channels: &[String],
) -> Result<HashMap<String, serde_json::Value>> {
    // We initialize the map to have an entry for every valid channel
    let mut channel_map = channels
        .iter()
        .map(|channel_name| (channel_name.clone(), json!({})))
        .collect::<HashMap<_, _>>();
    for default in defaults {
        if let Some(channels_for_default) = &default.merge_channels() {
            for channel in channels_for_default {
                if let Some(old_default) = channel_map.get(channel).cloned() {
                    if default.targeting.is_none() {
                        // TODO: we currently ignore any defaults with targeting involved
                        let merged = merge_two_defaults(&old_default, &default.value);
                        channel_map.insert(channel.clone(), merged);
                    }
                } else {
                    return Err(FMLError::InvalidChannelError(
                        channel.into(),
                        channels.into(),
                    ));
                }
            }
        // This is a default with no channel, so it applies to all channels
        } else {
            channel_map = channel_map
                .into_iter()
                .map(|(channel, old_default)| {
                    (channel, merge_two_defaults(&old_default, &default.value))
                })
                .collect();
        }
    }
    Ok(channel_map)
}

struct DefaultsMerger<'object> {
    defaults: HashMap<String, serde_json::Value>,
    objects: HashMap<String, &'object ObjectDef>,

    supported_channels: Vec<String>,
    channel: String,
}

impl<'object> DefaultsMerger<'object> {
    fn new(
        objects: HashMap<String, &'object ObjectDef>,
        supported_channels: Vec<String>,
        channel: String,
    ) -> Self {
        Self {
            objects,
            supported_channels,
            channel,

            defaults: Default::default(),
        }
    }

    fn collect_feature_defaults(&self, feature: &FeatureDef) -> Result<serde_json::Value> {
        let mut res = serde_json::value::Map::new();

        for p in feature.props() {
            let collected = self
                .collect_prop_defaults(&p.typ, &p.default)?
                .unwrap_or_else(|| p.default());
            res.insert(p.name(), collected);
        }

        Ok(serde_json::to_value(res)?)
    }

    fn collect_object_defaults(&self, nm: &str) -> Result<serde_json::Value> {
        if let Some(value) = self.defaults.get(nm) {
            return Ok(value.clone());
        }

        if !self.objects.contains_key(nm) {
            return Err(FMLError::ValidationError(
                format!("objects/{}", nm),
                format!("Object named {} is not defined", nm),
            ));
        }

        let obj = self.objects.get(nm).unwrap();
        let mut res = serde_json::value::Map::new();

        for p in obj.props() {
            if let Some(collected) = self.collect_prop_defaults(&p.typ, &p.default)? {
                res.insert(p.name(), collected);
            }
        }

        Ok(serde_json::to_value(res)?)
    }

    fn collect_prop_defaults(
        &self,
        typ: &TypeRef,
        v: &serde_json::Value,
    ) -> Result<Option<serde_json::Value>> {
        Ok(match typ {
            TypeRef::Object(nm) => Some(merge_two_defaults(&self.collect_object_defaults(nm)?, v)),
            TypeRef::EnumMap(_, v_type) => Some(self.collect_map_defaults(v_type, v)?),
            TypeRef::StringMap(v_type) => Some(self.collect_map_defaults(v_type, v)?),
            _ => None,
        })
    }

    fn collect_map_defaults(
        &self,
        v_type: &TypeRef,
        obj: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let map = obj
            .as_object()
            .unwrap_or_else(|| panic!("Expected a JSON object as a default"));
        let mut res = serde_json::value::Map::new();
        for (k, v) in map {
            let collected = self
                .collect_prop_defaults(v_type, v)?
                .unwrap_or_else(|| v.clone());
            res.insert(k.clone(), collected);
        }
        Ok(serde_json::to_value(res)?)
    }

    /// Transforms a feature definition with unmerged defaults into a feature
    /// definition with its defaults merged.
    ///
    /// # How the algorithm works:
    /// There are two types of defaults:
    /// 1. Field level defaults
    /// 1. Feature level defaults, that are listed by channel
    ///
    /// The algorithm gathers the field level defaults first, they are the base
    /// defaults. Then, it gathers the feature level defaults and merges them by
    /// calling [`collect_channel_defaults`]. Finally, it overwrites any common
    /// defaults between the merged feature level defaults and the field level defaults
    ///
    /// # Example:
    /// Assume we have the following feature manifest
    /// ```yaml
    ///  variables:
    ///   positive:
    ///   description: This is a positive button
    ///   type: Button
    ///   default:
    ///     {
    ///       "label": "Ok then",
    ///       "color": "blue"
    ///     }
    ///  default:
    ///      - channel: release
    ///      value: {
    ///        "positive": {
    ///          "color": "green"
    ///        }
    ///      }
    ///      - value: {
    ///      "positive": {
    ///        "alt-text": "Go Ahead!"
    ///      }
    /// }
    /// ```
    ///
    /// The result of the algorithm would be a default that looks like:
    /// ```yaml
    /// variables:
    ///     positive:
    ///     default:
    ///     {
    ///         "label": "Ok then",
    ///         "color": "green",
    ///         "alt-text": "Go Ahead!"
    ///     }
    ///
    /// ```
    ///
    /// - The `label` comes from the original field level default
    /// - The `color` comes from the `release` channel feature level default
    /// - The `alt-text` comes from the feature level default with no channel (that applies to all channels)
    ///
    /// # Arguments
    /// - `feature_def`: a [`FeatureDef`] representing the feature definition to transform
    /// - `channel`: a [`Option<&String>`] representing the channel to merge back into the field variables
    /// - `supported_channels`: a [`&[String]`] representing the channels that are supported by the manifest
    /// If the `channel` is `None` we default to using the `release` channel
    ///
    /// # Returns
    /// Returns a transformed [`FeatureDef`] with its defaults merged
    pub fn merge_feature_defaults(
        &self,
        feature_def: &mut FeatureDef,
        defaults: &Option<Vec<DefaultBlock>>,
    ) -> Result<(), FMLError> {
        let supported_channels = self.supported_channels.as_slice();
        let channel = &self.channel;
        if !supported_channels.iter().any(|c| c == channel) {
            return Err(FMLError::InvalidChannelError(
                channel.into(),
                supported_channels.into(),
            ));
        }
        let variable_defaults = self.collect_feature_defaults(feature_def)?;
        let mut res = feature_def;

        if let Some(defaults) = defaults {
            let merged_defaults = collect_channel_defaults(defaults, supported_channels)?;
            if let Some(default_to_merged) = merged_defaults.get(channel) {
                let merged = merge_two_defaults(&variable_defaults, default_to_merged);
                let map = merged.as_object().ok_or(FMLError::InternalError(
                    "Map was merged into a different type",
                ))?;
                let new_props = res
                    .props
                    .iter()
                    .map(|prop| {
                        let mut res = prop.clone();
                        if let Some(default) = map.get(&prop.name).cloned() {
                            res.default = default
                        }
                        res
                    })
                    .collect::<Vec<_>>();

                res.props = new_props;
            }
        }
        Ok(())
    }
}

/// Merges two [`serde_json::Value`]s into one
///
/// # Arguments:
/// - `old_default`: a reference to a [`serde_json::Value`], that represents the old default
/// - `new_default`: a reference to a [`serde_json::Value`], that represents the new default, this takes
///     precedence over the `old_default` if they have conflicting fields
///
/// # Returns
/// A merged [`serde_json::Value`] that contains all fields from `old_default` and `new_default`, merging
/// where there is a conflict. If the `old_default` and `new_default` are not both objects, this function
/// returns the `new_default`
fn merge_two_defaults(
    old_default: &serde_json::Value,
    new_default: &serde_json::Value,
) -> serde_json::Value {
    use serde_json::Value::Object;
    match (old_default.clone(), new_default.clone()) {
        (Object(old), Object(new)) => {
            let mut merged = serde_json::Map::new();
            for (key, val) in old {
                merged.insert(key, val);
            }
            for (key, val) in new {
                if let Some(old_val) = merged.get(&key).cloned() {
                    merged.insert(key, merge_two_defaults(&old_val, &val));
                } else {
                    merged.insert(key, val);
                }
            }
            Object(merged)
        }
        (_, new) => new,
    }
}

fn get_typeref_from_string(
    input: String,
    types: Option<HashMap<String, TypeRef>>,
) -> Result<TypeRef, FMLError> {
    let (type_ref, type_name) = parse_typeref_string(input)?;

    return match type_ref.as_str() {
        "String" => Ok(TypeRef::String),
        "Int" => Ok(TypeRef::Int),
        "Boolean" => Ok(TypeRef::Boolean),
        "BundleText" | "Text" => Ok(TypeRef::BundleText(
            type_name.unwrap_or_else(|| "unnamed".to_string()),
        )),
        "BundleImage" | "Drawable" | "Image" => Ok(TypeRef::BundleImage(
            type_name.unwrap_or_else(|| "unnamed".to_string()),
        )),
        "Enum" => Ok(TypeRef::Enum(type_name.unwrap())),
        "Object" => Ok(TypeRef::Object(type_name.unwrap())),
        "List" => Ok(TypeRef::List(Box::new(get_typeref_from_string(
            type_name.unwrap(),
            types,
        )?))),
        "Option" => Ok(TypeRef::Option(Box::new(get_typeref_from_string(
            type_name.unwrap(),
            types,
        )?))),
        "Map" => {
            // Maps take a little extra massaging to get the key and value types
            let type_name = type_name.unwrap();
            let mut map_type_info_iter = type_name.split(',');

            let key_type = map_type_info_iter.next().unwrap().to_string();
            let value_type = map_type_info_iter.next().unwrap().trim().to_string();

            if key_type.eq("String") {
                Ok(TypeRef::StringMap(Box::new(get_typeref_from_string(
                    value_type, types,
                )?)))
            } else {
                Ok(TypeRef::EnumMap(
                    Box::new(get_typeref_from_string(key_type, types.clone())?),
                    Box::new(get_typeref_from_string(value_type, types)?),
                ))
            }
        }
        type_name => {
            if types.is_none() {
                return Err(FMLError::TypeParsingError(format!(
                    "{} is not a recognized FML type",
                    type_ref
                )));
            }

            match types.unwrap().get(type_name) {
                Some(type_ref) => Ok(type_ref.clone()),
                None => {
                    return Err(FMLError::TypeParsingError(format!(
                        "{} is not a recognized FML type",
                        type_ref
                    )));
                }
            }
        }
    };
}
#[derive(Debug)]
pub struct Parser {
    files: FileLoader,
    source: FilePath,
}

impl Parser {
    pub fn new(files: FileLoader, source: FilePath) -> Result<Parser> {
        Ok(Parser { source, files })
    }

    // This method loads a manifest, including resolving the includes and merging the included files
    // into this top level one.
    // It recursively calls itself and then calls `merge_manifest`.
    fn load_manifest(
        &self,
        path: &FilePath,
        loading: &mut HashSet<ModuleId>,
    ) -> Result<ManifestFrontEnd> {
        let id: ModuleId = path.try_into()?;
        let files = &self.files;
        let s = files
            .read_to_string(path)
            .map_err(|e| FMLError::FMLModuleError(id.clone(), e.to_string()))?;

        let mut parent = serde_yaml::from_str::<ManifestFrontEnd>(&s)
            .map_err(|e| FMLError::FMLModuleError(id.clone(), e.to_string()))?;

        // We canonicalize the paths to the import files really soon after the loading so when we merge
        // other included files, we cam match up the files that _they_ import, the concatenate the default
        // blocks for their features.
        self.canonicalize_import_paths(path, &mut parent.imports)
            .map_err(|e| FMLError::FMLModuleError(id.clone(), e.to_string()))?;

        loading.insert(id.clone());
        parent
            .includes
            .clone()
            .iter()
            .fold(Ok(parent), |parent: Result<ManifestFrontEnd>, f| {
                let src_path = files.join(path, f)?;
                let parent = parent?;
                let child_id = ModuleId::try_from(&src_path)?;
                Ok(if !loading.contains(&child_id) {
                    let manifest = self.load_manifest(&src_path, loading)?;
                    self.merge_manifest(&src_path, parent, &src_path, manifest)
                        .map_err(|e| FMLError::FMLModuleError(id.clone(), e.to_string()))?
                } else {
                    parent
                })
            })
    }

    // Attempts to merge two manifests: a child into a parent.
    // The `child_path` is needed to report errors.
    fn merge_manifest(
        &self,
        parent_path: &FilePath,
        parent: ManifestFrontEnd,
        child_path: &FilePath,
        child: ManifestFrontEnd,
    ) -> Result<ManifestFrontEnd> {
        self.check_can_merge_manifest(parent_path, &parent, child_path, &child)?;

        // Child must not specify any features, objects or enums that the parent has.
        let features = merge_map(
            &parent.features,
            &child.features,
            "Features",
            "features",
            child_path,
        )?;

        let p_types = &parent.legacy_types.unwrap_or(parent.types);
        let c_types = &child.legacy_types.unwrap_or(child.types);

        let objects = merge_map(
            &c_types.objects,
            &p_types.objects,
            "Objects",
            "objects",
            child_path,
        )?;
        let enums = merge_map(&c_types.enums, &p_types.enums, "Enums", "enums", child_path)?;

        let imports = self.merge_import_block_list(&parent.imports, &child.imports)?;

        let merged = ManifestFrontEnd {
            features,
            types: Types { enums, objects },
            legacy_types: None,
            imports,
            ..parent
        };

        Ok(merged)
    }

    /// Load a manifest and all its imports, recursively if necessary.
    ///
    /// We populate a map of `FileId` to `FeatureManifest`s, so to avoid unnecessary clones,
    /// we return a `FileId` even when the file has already been imported.
    fn load_imports(
        &self,
        current: &FilePath,
        channel: &str,
        imports: &mut HashMap<ModuleId, FeatureManifest>,
    ) -> Result<ModuleId> {
        let id = current.try_into()?;
        if imports.contains_key(&id) {
            return Ok(id);
        }
        // We put a terminus in here, to make sure we don't try and load more than once.
        imports.insert(id.clone(), Default::default());

        // This loads the manifest in its frontend format (i.e. direct from YAML via serde), including
        // all the `includes` for this manifest.
        let frontend = self.load_manifest(current, &mut HashSet::new())?;

        // Aside: tiny quality of life improvement. In the case where only one channel is supported,
        // we use it. This helps with globbing directories where the app wants to keep the feature definition
        // away from the feature configuration.
        let channel = if frontend.channels.len() == 1 {
            frontend.channels.first().unwrap()
        } else {
            channel
        };

        let mut manifest = frontend.get_intermediate_representation(&id, channel)?;

        // We're now going to go through all the imports in the manifest YAML.
        // Each of the import blocks will have a path, and a Map<FeatureId, List<DefaultBlock>>
        // This loop does the work of merging the default blocks back into the imported manifests.
        // We'll then attach all the manifests to the root (i.e. the one we're generating code for today), in `imports`.
        // We associate only the feature ids with the manifest we're loading in this method.
        let mut imported_feature_id_map = HashMap::new();

        for block in &frontend.imports {
            // 1. Load the imported manifests in to the hash map.
            let path = self.files.join(current, &block.path)?;
            // The channel comes from the importer, rather than the command or the imported file.
            let child_id = self.load_imports(&path, &block.channel, imports)?;
            let child_manifest = imports.get_mut(&child_id).expect("just loaded this file");

            // We detect that there are no name collisions after the loading has finished, with `check_can_import_manifest`.
            // We can't do it greedily, because of transitive imports may cause collisions, but we'll check here for better error
            // messages.
            check_can_import_manifest(&manifest, child_manifest)?;

            // We detect that the imported files have language specific files in `validate_manifest_for_lang()`.
            // We can't do it now because we don't yet know what this run is going to generate.

            // 2. We'll build a set of feature names that this manifest imports from the child manifest.
            // This will be the only thing we add directly to the manifest we load in this method.
            let mut feature_ids = BTreeSet::new();

            // 3. For each of the features in each of the imported files, the user can specify new defaults that should
            //    merge into/overwrite the defaults specified in the imported file. Let's do that now:
            // a. Prepare a DefaultsMerger, with an object map.
            let object_map: HashMap<String, &ObjectDef> = child_manifest
                .obj_defs
                .iter()
                .map(|o| (o.name(), o))
                .collect();
            let merger = DefaultsMerger::new(object_map, frontend.channels.clone(), channel.into());

            // b. Prepare a feature map that we'll alter in place.
            //    EXP- 2540 If we want to support re-exporting/encapsulating features then we will need to change
            //    this to be a more recursive look up. e.g. change `FeatureManifest.feature_defs` to be a `BTreeMap`.
            let mut feature_map: HashMap<String, &mut FeatureDef> = child_manifest
                .feature_defs
                .iter_mut()
                .map(|o| (o.name(), o))
                .collect();

            // c. Iterate over the features we want to override
            for (f, default_blocks) in &block.features {
                let feature_def = feature_map.get_mut(f).ok_or_else(|| {
                    FMLError::FMLModuleError(
                        id.clone(),
                        format!(
                            "Cannot override defaults for `{}` feature from {}",
                            f, &child_id
                        ),
                    )
                })?;

                // d. And merge the overrides in place into the FeatureDefs
                merger
                    .merge_feature_defaults(feature_def, &Some(default_blocks).cloned())
                    .map_err(|e| FMLError::FMLModuleError(child_id.clone(), e.to_string()))?;

                feature_ids.insert(f.clone());
            }

            // 4. Associate the imports as children of this manifest.
            imported_feature_id_map.insert(child_id.clone(), feature_ids);
        }

        manifest.imported_features = imported_feature_id_map;
        imports.insert(id.clone(), manifest);

        Ok(id)
    }

    pub fn get_intermediate_representation(
        &self,
        channel: &str,
    ) -> Result<FeatureManifest, FMLError> {
        let mut manifests = HashMap::new();
        let id = self.load_imports(&self.source, channel, &mut manifests)?;
        let mut fm = manifests
            .remove(&id)
            .expect("Top level manifest should always be present");

        for child in manifests.values() {
            check_can_import_manifest(&fm, child)?;
        }

        fm.all_imports = manifests;

        Ok(fm)
    }
}

impl Parser {
    fn check_can_merge_manifest(
        &self,
        parent_path: &FilePath,
        parent: &ManifestFrontEnd,
        child_path: &FilePath,
        child: &ManifestFrontEnd,
    ) -> Result<()> {
        if !child.channels.is_empty() {
            let child = &child.channels;
            let child = child.iter().collect::<HashSet<&String>>();
            let parent = &parent.channels;
            let parent = parent.iter().collect::<HashSet<&String>>();
            if !child.is_subset(&parent) {
                return Err(FMLError::ValidationError(
                    "channels".to_string(),
                    format!(
                        "Included manifest should not define its own channels: {}",
                        child_path
                    ),
                ));
            }
        }

        if let Some(about) = &child.about {
            if !about.is_includable() {
                return Err(FMLError::ValidationError(
                "about".to_string(),
                format!("Only files that don't already correspond to generated files may be included: file has a `class` and `package`/`module` name: {}", child_path),
            ));
            }
        }

        let mut map = Default::default();
        self.check_can_merge_imports(parent_path, &parent.imports, &mut map)?;
        self.check_can_merge_imports(child_path, &child.imports, &mut map)?;

        Ok(())
    }

    fn canonicalize_import_paths(
        &self,
        path: &FilePath,
        blocks: &mut Vec<ImportBlock>,
    ) -> Result<()> {
        for ib in blocks {
            let p = &self.files.join(path, &ib.path)?;
            ib.path = p.canonicalize()?.to_string();
        }
        Ok(())
    }

    fn check_can_merge_imports(
        &self,
        path: &FilePath,
        blocks: &Vec<ImportBlock>,
        map: &mut HashMap<String, String>,
    ) -> Result<()> {
        for b in blocks {
            let id = &b.path;
            let channel = &b.channel;
            let existing = map.insert(id.clone(), channel.clone());
            if let Some(v) = existing {
                if &v != channel {
                    return Err(FMLError::FMLModuleError(
                        path.try_into()?,
                        format!(
                            "File {} is imported with two different channels: {} and {}",
                            id, v, &channel
                        ),
                    ));
                }
            }
        }
        Ok(())
    }

    fn merge_import_block_list(
        &self,
        parent: &[ImportBlock],
        child: &[ImportBlock],
    ) -> Result<Vec<ImportBlock>> {
        let mut map = parent
            .iter()
            .map(|im| (im.path.clone(), im.clone()))
            .collect::<HashMap<_, _>>();

        for cib in child {
            let path = &cib.path;
            if let Some(pib) = map.get(path) {
                // We'll define an ordering here: the parent will come after the child
                // so the top-level one will override the lower level ones.
                // In practice, this shouldn't make a difference.
                let merged = merge_import_block(cib, pib)?;
                map.insert(path.clone(), merged);
            } else {
                map.insert(path.clone(), cib.clone());
            }
        }

        Ok(map.values().map(|b| b.to_owned()).collect::<Vec<_>>())
    }
}

fn merge_map<T: Clone>(
    a: &HashMap<String, T>,
    b: &HashMap<String, T>,
    display_key: &str,
    key: &str,
    child_path: &FilePath,
) -> Result<HashMap<String, T>> {
    let mut set = HashSet::new();

    let (a, b) = if a.len() < b.len() { (a, b) } else { (b, a) };

    let mut map = b.clone();

    for (k, v) in a {
        if map.contains_key(k) {
            set.insert(k.clone());
        } else {
            map.insert(k.clone(), v.clone());
        }
    }

    if set.is_empty() {
        Ok(map)
    } else {
        Err(FMLError::ValidationError(
            format!("{}/{:?}", key, set),
            format!(
                "{} cannot be defined twice, overloaded definition detected at {}",
                display_key, child_path,
            ),
        ))
    }
}

fn merge_import_block(a: &ImportBlock, b: &ImportBlock) -> Result<ImportBlock> {
    let mut block = a.clone();

    for (id, defaults) in &b.features {
        let mut defaults = defaults.clone();
        if let Some(existing) = block.features.get_mut(id) {
            existing.append(&mut defaults);
        } else {
            block.features.insert(id.clone(), defaults.clone());
        }
    }
    Ok(block)
}

/// Check if this parent can import this child.
fn check_can_import_manifest(parent: &FeatureManifest, child: &FeatureManifest) -> Result<()> {
    check_can_import_list(parent, child, "enum", |fm: &FeatureManifest| {
        fm.iter_enum_defs()
            .map(|e| &e.name)
            .collect::<HashSet<&String>>()
    })?;
    check_can_import_list(parent, child, "objects", |fm: &FeatureManifest| {
        fm.iter_object_defs()
            .map(|o| &o.name)
            .collect::<HashSet<&String>>()
    })?;
    check_can_import_list(parent, child, "features", |fm: &FeatureManifest| {
        fm.iter_feature_defs()
            .map(|f| &f.name)
            .collect::<HashSet<&String>>()
    })?;

    Ok(())
}

fn check_can_import_list(
    parent: &FeatureManifest,
    child: &FeatureManifest,
    key: &str,
    f: fn(&FeatureManifest) -> HashSet<&String>,
) -> Result<()> {
    let p = f(parent);
    let c = f(child);
    let intersection = p.intersection(&c).collect::<HashSet<_>>();
    if !intersection.is_empty() {
        Err(FMLError::ValidationError(
            key.to_string(),
            format!(
                "`{}` types {:?} conflict when {} imports {}",
                key, &intersection, &parent.id, &child.id
            ),
        ))
    } else {
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct DefaultBlock {
    #[serde(skip_serializing_if = "Option::is_none")]
    channel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    channels: Option<Vec<String>>,
    value: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    targeting: Option<String>,
}

impl DefaultBlock {
    pub fn merge_channels(&self) -> Option<Vec<String>> {
        let mut res = HashSet::new();

        if let Some(channels) = self.channels.clone() {
            res.extend(channels)
        }

        if let Some(channel) = &self.channel {
            res.extend(
                channel
                    .split(',')
                    .filter(|channel_name| !channel_name.is_empty())
                    .map(|channel_name| channel_name.trim().to_string())
                    .collect::<HashSet<String>>(),
            )
        }

        let res: Vec<String> = res.into_iter().collect();
        if res.is_empty() {
            None
        } else {
            Some(res)
        }
    }
}

#[cfg(test)]
mod default_block {
    use super::*;

    #[test]
    fn test_merge_channels_none_when_empty() {
        let input: DefaultBlock = serde_json::from_value(json!(
            {
                "channel": "",
                "channels": [],
                "value": {
                    "button-color": "green"
                }
            }
        ))
        .unwrap();
        assert!(input.merge_channels() == None)
    }

    #[test]
    fn test_merge_channels_merged_when_present() {
        let input: DefaultBlock = serde_json::from_value(json!(
            {
                "channel": "a, b",
                "channels": ["c"],
                "value": {
                    "button-color": "green"
                }
            }
        ))
        .unwrap();
        let res = input.merge_channels();
        assert!(res != None);
        let res = res.unwrap();
        assert!(res.contains(&"a".to_string()));
        assert!(res.contains(&"b".to_string()));
        assert!(res.contains(&"c".to_string()));
    }

    #[test]
    fn test_merge_channels_merged_without_duplicates() {
        let input: DefaultBlock = serde_json::from_value(json!(
            {
                "channel": "a, a",
                "channels": ["a"],
                "value": {
                    "button-color": "green"
                }
            }
        ))
        .unwrap();
        let res = input.merge_channels();
        assert!(res != None);
        let res = res.unwrap();
        assert!(res.contains(&"a".to_string()));
        assert!(res.len() == 1)
    }
}

#[cfg(test)]
mod unit_tests {

    use std::{
        path::{Path, PathBuf},
        vec,
    };

    use super::*;
    use crate::{
        error::Result,
        util::{join, pkg_dir},
    };

    #[test]
    fn test_parse_from_front_end_representation() -> Result<()> {
        let path = join(pkg_dir(), "fixtures/fe/nimbus_features.yaml");
        let path = Path::new(&path);
        let files = FileLoader::default()?;
        let parser = Parser::new(files, path.into())?;
        let ir = parser.get_intermediate_representation("release")?;

        // Validate parsed enums
        assert!(ir.enum_defs.len() == 1);
        let enum_def = ir.enum_defs.first().unwrap();
        assert!(enum_def.name == *"PlayerProfile");
        assert!(enum_def.doc == *"This is an enum type");
        assert!(enum_def.variants.contains(&VariantDef {
            name: "adult".to_string(),
            doc: "This represents an adult player profile".to_string()
        }));
        assert!(enum_def.variants.contains(&VariantDef {
            name: "child".to_string(),
            doc: "This represents a child player profile".to_string()
        }));

        // Validate parsed objects
        assert!(ir.obj_defs.len() == 1);
        let obj_def = ir.obj_defs.first().unwrap();
        assert!(obj_def.name == *"Button");
        assert!(obj_def.doc == *"This is a button object");
        assert!(obj_def.props.contains(&PropDef {
            name: "label".to_string(),
            doc: "This is the label for the button".to_string(),
            typ: TypeRef::String,
            default: serde_json::Value::String("REQUIRED FIELD".to_string()),
        }));
        assert!(obj_def.props.contains(&PropDef {
            name: "color".to_string(),
            doc: "This is the color of the button".to_string(),
            typ: TypeRef::Option(Box::new(TypeRef::String)),
            default: serde_json::Value::Null,
        }));

        // Validate parsed features
        assert!(ir.feature_defs.len() == 1);
        // assert!(ir.feature_defs.contains(parser.features.first().unwrap()));
        let feature_def = ir.feature_defs.first().unwrap();
        assert!(feature_def.name == *"dialog-appearance");
        assert!(feature_def.doc == *"This is the appearance of the dialog");
        let positive_button = feature_def
            .props
            .iter()
            .find(|x| x.name == "positive")
            .unwrap();
        assert!(positive_button.name == *"positive");
        assert!(positive_button.doc == *"This is a positive button");
        assert!(positive_button.typ == TypeRef::Object("Button".to_string()));
        // We verify that the label, which came from the field default is "Ok then"
        // and the color default, which came from the feature default is "green"
        assert!(positive_button.default.get("label").unwrap().as_str() == Some("Ok then"));
        assert!(positive_button.default.get("color").unwrap().as_str() == Some("green"));
        let negative_button = feature_def
            .props
            .iter()
            .find(|x| x.name == "negative")
            .unwrap();
        assert!(negative_button.name == *"negative");
        assert!(negative_button.doc == *"This is a negative button");
        assert!(negative_button.typ == TypeRef::Object("Button".to_string()));
        assert!(negative_button.default.get("label").unwrap().as_str() == Some("Not this time"));
        assert!(negative_button.default.get("color").unwrap().as_str() == Some("red"));
        let background_color = feature_def
            .props
            .iter()
            .find(|x| x.name == "background-color")
            .unwrap();
        assert!(background_color.name == *"background-color");
        assert!(background_color.doc == *"This is the background color");
        assert!(background_color.typ == TypeRef::String);
        assert!(background_color.default.as_str() == Some("white"));
        let player_mapping = feature_def
            .props
            .iter()
            .find(|x| x.name == "player-mapping")
            .unwrap();
        assert!(player_mapping.name == *"player-mapping");
        assert!(player_mapping.doc == *"This is the map of the player type to a button");
        assert!(
            player_mapping.typ
                == TypeRef::EnumMap(
                    Box::new(TypeRef::Enum("PlayerProfile".to_string())),
                    Box::new(TypeRef::Object("Button".to_string()))
                )
        );
        assert!(
            player_mapping.default
                == json!({
                    "child": {
                        "label": "Play game!",
                        "color": "green"
                    },
                    "adult": {
                        "label": "Play game!",
                        "color": "blue",
                    }
                })
        );

        Ok(())
    }

    #[test]
    fn test_parsing_about_block() -> Result<()> {
        let about = AboutBlock {
            kotlin_about: Some(KotlinAboutBlock {
                package: "com.example".to_string(),
                class: "KotlinAbout".to_string(),
            }),
            ..Default::default()
        };

        let yaml = serde_yaml::to_value(&about)?;

        let rehydrated = serde_yaml::from_value(yaml)?;

        assert_eq!(about, rehydrated);

        Ok(())
    }

    #[test]
    fn test_merging_defaults() -> Result<()> {
        let path = join(pkg_dir(), "fixtures/fe/default_merging.yaml");
        let path = Path::new(&path);
        let files = FileLoader::default()?;
        let parser = Parser::new(files, path.into())?;
        let ir = parser.get_intermediate_representation("release")?;
        let feature_def = ir.feature_defs.first().unwrap();
        let positive_button = feature_def
            .props
            .iter()
            .find(|x| x.name == "positive")
            .unwrap();
        // We validate that the no-channel feature level default got merged back
        assert_eq!(
            positive_button
                .default
                .get("alt-text")
                .unwrap()
                .as_str()
                .unwrap(),
            "Go Ahead!"
        );
        // We validate that the orignal field level default don't get lost if no
        // feature level default with the same name exists
        assert_eq!(
            positive_button
                .default
                .get("label")
                .unwrap()
                .as_str()
                .unwrap(),
            "Ok then"
        );
        // We validate that feature level default overwrite field level defaults if one exists
        // in the field level, it's blue, but on the feature level it's green
        assert_eq!(
            positive_button
                .default
                .get("color")
                .unwrap()
                .as_str()
                .unwrap(),
            "green"
        );
        // We now re-run this, but merge back the nightly channel instead
        let files = FileLoader::default()?;
        let parser = Parser::new(files, path.into())?;
        let ir = parser.get_intermediate_representation("nightly")?;
        let feature_def = ir.feature_defs.first().unwrap();
        let positive_button = feature_def
            .props
            .iter()
            .find(|x| x.name == "positive")
            .unwrap();
        // We validate that feature level default overwrite field level defaults if one exists
        // in the field level, it's blue, but on the feature level it's bright-red
        // note that it's bright-red because we merged back the `nightly`
        // channel, instead of the `release` channel that merges back
        // by default
        assert_eq!(
            positive_button
                .default
                .get("color")
                .unwrap()
                .as_str()
                .unwrap(),
            "bright-red"
        );
        // We againt validate that regardless
        // of the channel, the no-channel feature level default got merged back
        assert_eq!(
            positive_button
                .default
                .get("alt-text")
                .unwrap()
                .as_str()
                .unwrap(),
            "Go Ahead!"
        );
        Ok(())
    }

    #[test]
    fn test_convert_to_typeref_string() -> Result<()> {
        // Testing converting to TypeRef::String
        assert_eq!(
            get_typeref_from_string("String".to_string(), None).unwrap(),
            TypeRef::String
        );
        get_typeref_from_string("string".to_string(), None).unwrap_err();
        get_typeref_from_string("str".to_string(), None).unwrap_err();

        Ok(())
    }

    #[test]
    fn test_convert_to_typeref_int() -> Result<()> {
        // Testing converting to TypeRef::Int
        assert_eq!(
            get_typeref_from_string("Int".to_string(), None).unwrap(),
            TypeRef::Int
        );
        get_typeref_from_string("integer".to_string(), None).unwrap_err();
        get_typeref_from_string("int".to_string(), None).unwrap_err();

        Ok(())
    }

    #[test]
    fn test_convert_to_typeref_boolean() -> Result<()> {
        // Testing converting to TypeRef::Boolean
        assert_eq!(
            get_typeref_from_string("Boolean".to_string(), None).unwrap(),
            TypeRef::Boolean
        );
        get_typeref_from_string("boolean".to_string(), None).unwrap_err();
        get_typeref_from_string("bool".to_string(), None).unwrap_err();

        Ok(())
    }

    #[test]
    fn test_convert_to_typeref_bundletext() -> Result<()> {
        // Testing converting to TypeRef::BundleText
        assert_eq!(
            get_typeref_from_string("BundleText<test_name>".to_string(), None).unwrap(),
            TypeRef::BundleText("test_name".to_string())
        );
        get_typeref_from_string("bundletext(something)".to_string(), None).unwrap_err();
        get_typeref_from_string("BundleText()".to_string(), None).unwrap_err();

        // The commented out lines below represent areas we need better
        // type checking on, but are ignored for now

        // get_typeref_from_string("BundleText".to_string()).unwrap_err();
        // get_typeref_from_string("BundleText<>".to_string()).unwrap_err();
        // get_typeref_from_string("BundleText<21>".to_string()).unwrap_err();

        Ok(())
    }

    #[test]
    fn test_convert_to_typeref_bundleimage() -> Result<()> {
        // Testing converting to TypeRef::BundleImage
        assert_eq!(
            get_typeref_from_string("BundleImage<test_name>".to_string(), None).unwrap(),
            TypeRef::BundleImage("test_name".to_string())
        );
        get_typeref_from_string("bundleimage(something)".to_string(), None).unwrap_err();
        get_typeref_from_string("BundleImage()".to_string(), None).unwrap_err();

        // The commented out lines below represent areas we need better
        // type checking on, but are ignored for now

        // get_typeref_from_string("BundleImage".to_string()).unwrap_err();
        // get_typeref_from_string("BundleImage<>".to_string()).unwrap_err();
        // get_typeref_from_string("BundleImage<21>".to_string()).unwrap_err();

        Ok(())
    }

    #[test]
    fn test_convert_to_typeref_enum() -> Result<()> {
        // Testing converting to TypeRef::Enum
        assert_eq!(
            get_typeref_from_string("Enum<test_name>".to_string(), None).unwrap(),
            TypeRef::Enum("test_name".to_string())
        );
        get_typeref_from_string("enum(something)".to_string(), None).unwrap_err();
        get_typeref_from_string("Enum()".to_string(), None).unwrap_err();

        // The commented out lines below represent areas we need better
        // type checking on, but are ignored for now

        // get_typeref_from_string("Enum".to_string()).unwrap_err();
        // get_typeref_from_string("Enum<>".to_string()).unwrap_err();
        // get_typeref_from_string("Enum<21>".to_string()).unwrap_err();

        Ok(())
    }

    #[test]
    fn test_convert_to_typeref_object() -> Result<()> {
        // Testing converting to TypeRef::Object
        assert_eq!(
            get_typeref_from_string("Object<test_name>".to_string(), None).unwrap(),
            TypeRef::Object("test_name".to_string())
        );
        get_typeref_from_string("object(something)".to_string(), None).unwrap_err();
        get_typeref_from_string("Object()".to_string(), None).unwrap_err();

        // The commented out lines below represent areas we need better
        // type checking on, but are ignored for now

        // get_typeref_from_string("Object".to_string()).unwrap_err();
        // get_typeref_from_string("Object<>".to_string()).unwrap_err();
        // get_typeref_from_string("Object<21>".to_string()).unwrap_err();

        Ok(())
    }

    #[test]
    fn test_convert_to_typeref_list() -> Result<()> {
        // Testing converting to TypeRef::List
        assert_eq!(
            get_typeref_from_string("List<String>".to_string(), None).unwrap(),
            TypeRef::List(Box::new(TypeRef::String))
        );
        assert_eq!(
            get_typeref_from_string("List<Int>".to_string(), None).unwrap(),
            TypeRef::List(Box::new(TypeRef::Int))
        );
        assert_eq!(
            get_typeref_from_string("List<Boolean>".to_string(), None).unwrap(),
            TypeRef::List(Box::new(TypeRef::Boolean))
        );

        // Generate a list of user types to validate use of them in a list
        let mut types = HashMap::new();
        types.insert(
            "TestEnum".to_string(),
            TypeRef::Enum("TestEnum".to_string()),
        );
        types.insert(
            "TestObject".to_string(),
            TypeRef::Object("TestObject".to_string()),
        );

        assert_eq!(
            get_typeref_from_string("List<TestEnum>".to_string(), Some(types.clone())).unwrap(),
            TypeRef::List(Box::new(TypeRef::Enum("TestEnum".to_string())))
        );
        assert_eq!(
            get_typeref_from_string("List<TestObject>".to_string(), Some(types)).unwrap(),
            TypeRef::List(Box::new(TypeRef::Object("TestObject".to_string())))
        );

        get_typeref_from_string("list(something)".to_string(), None).unwrap_err();
        get_typeref_from_string("List()".to_string(), None).unwrap_err();

        // The commented out lines below represent areas we need better
        // type checking on, but are ignored for now

        // get_typeref_from_string("List".to_string()).unwrap_err();
        // get_typeref_from_string("List<>".to_string()).unwrap_err();
        // get_typeref_from_string("List<21>".to_string()).unwrap_err();

        Ok(())
    }

    #[test]
    fn test_convert_to_typeref_option() -> Result<()> {
        // Testing converting to TypeRef::Option
        assert_eq!(
            get_typeref_from_string("Option<String>".to_string(), None).unwrap(),
            TypeRef::Option(Box::new(TypeRef::String))
        );
        assert_eq!(
            get_typeref_from_string("Option<Int>".to_string(), None).unwrap(),
            TypeRef::Option(Box::new(TypeRef::Int))
        );
        assert_eq!(
            get_typeref_from_string("Option<Boolean>".to_string(), None).unwrap(),
            TypeRef::Option(Box::new(TypeRef::Boolean))
        );

        // Generate a list of user types to validate use of them as Options
        let mut types = HashMap::new();
        types.insert(
            "TestEnum".to_string(),
            TypeRef::Enum("TestEnum".to_string()),
        );
        types.insert(
            "TestObject".to_string(),
            TypeRef::Object("TestObject".to_string()),
        );
        assert_eq!(
            get_typeref_from_string("Option<TestEnum>".to_string(), Some(types.clone())).unwrap(),
            TypeRef::Option(Box::new(TypeRef::Enum("TestEnum".to_string())))
        );
        assert_eq!(
            get_typeref_from_string("Option<TestObject>".to_string(), Some(types)).unwrap(),
            TypeRef::Option(Box::new(TypeRef::Object("TestObject".to_string())))
        );

        get_typeref_from_string("option(something)".to_string(), None).unwrap_err();
        get_typeref_from_string("Option(Something)".to_string(), None).unwrap_err();

        // The commented out lines below represent areas we need better
        // type checking on, but are ignored for now

        // get_typeref_from_string("Option".to_string()).unwrap_err();
        // get_typeref_from_string("Option<>".to_string()).unwrap_err();
        // get_typeref_from_string("Option<21>".to_string()).unwrap_err();

        Ok(())
    }

    #[test]
    fn test_convert_to_typeref_map() -> Result<()> {
        // Testing converting to TypeRef::Map
        assert_eq!(
            get_typeref_from_string("Map<String, String>".to_string(), None).unwrap(),
            TypeRef::StringMap(Box::new(TypeRef::String))
        );
        assert_eq!(
            get_typeref_from_string("Map<String, Int>".to_string(), None).unwrap(),
            TypeRef::StringMap(Box::new(TypeRef::Int))
        );
        assert_eq!(
            get_typeref_from_string("Map<String, Boolean>".to_string(), None).unwrap(),
            TypeRef::StringMap(Box::new(TypeRef::Boolean))
        );

        // Generate a list of user types to validate use of them in a list
        let mut types = HashMap::new();
        types.insert(
            "TestEnum".to_string(),
            TypeRef::Enum("TestEnum".to_string()),
        );
        types.insert(
            "TestObject".to_string(),
            TypeRef::Object("TestObject".to_string()),
        );
        assert_eq!(
            get_typeref_from_string("Map<String, TestEnum>".to_string(), Some(types.clone()))
                .unwrap(),
            TypeRef::StringMap(Box::new(TypeRef::Enum("TestEnum".to_string())))
        );
        assert_eq!(
            get_typeref_from_string("Map<String, TestObject>".to_string(), Some(types.clone()))
                .unwrap(),
            TypeRef::StringMap(Box::new(TypeRef::Object("TestObject".to_string())))
        );
        assert_eq!(
            get_typeref_from_string("Map<TestEnum, String>".to_string(), Some(types.clone()))
                .unwrap(),
            TypeRef::EnumMap(
                Box::new(TypeRef::Enum("TestEnum".to_string())),
                Box::new(TypeRef::String)
            )
        );
        assert_eq!(
            get_typeref_from_string("Map<TestEnum, TestObject>".to_string(), Some(types.clone()))
                .unwrap(),
            TypeRef::EnumMap(
                Box::new(TypeRef::Enum("TestEnum".to_string())),
                Box::new(TypeRef::Object("TestObject".to_string()))
            )
        );

        get_typeref_from_string("map(something)".to_string(), None).unwrap_err();
        get_typeref_from_string("Map(Something)".to_string(), None).unwrap_err();

        // The commented out lines below represent areas we need better
        // type checking on, but are ignored for now

        // get_typeref_from_string("Map".to_string()).unwrap_err();
        // get_typeref_from_string("Map<>".to_string()).unwrap_err();
        // get_typeref_from_string("Map<21>".to_string()).unwrap_err();

        Ok(())
    }

    #[test]
    fn test_merge_two_defaults_both_objects_no_intersection() -> Result<()> {
        let old_default = json!({
            "button-color": "blue",
            "dialog_option": "greetings",
            "is_enabled": false,
            "num_items": 5
        });
        let new_default = json!({
            "new_homepage": true,
            "item_order": ["first", "second", "third"],
        });
        let merged = merge_two_defaults(&old_default, &new_default);
        assert_eq!(
            json!({
                "button-color": "blue",
                "dialog_option": "greetings",
                "is_enabled": false,
                "num_items": 5,
                "new_homepage": true,
                "item_order": ["first", "second", "third"],
            }),
            merged
        );
        Ok(())
    }

    #[test]
    fn test_merge_two_defaults_intersecting_different_types() -> Result<()> {
        // if there is an intersection, but they are different types, we just take the new one
        let old_default = json!({
            "button-color": "blue",
            "dialog_option": "greetings",
            "is_enabled": {
                "value": false
            },
            "num_items": 5
        });
        let new_default = json!({
            "new_homepage": true,
            "is_enabled": true,
            "item_order": ["first", "second", "third"],
        });
        let merged = merge_two_defaults(&old_default, &new_default);
        assert_eq!(
            json!({
                "button-color": "blue",
                "dialog_option": "greetings",
                "is_enabled": true,
                "num_items": 5,
                "new_homepage": true,
                "item_order": ["first", "second", "third"],
            }),
            merged
        );
        Ok(())
    }

    #[test]
    fn test_merge_two_defaults_non_map_intersection() -> Result<()> {
        // if they intersect on both key and type, but the type intersected is not an object, we just take the new one
        let old_default = json!({
            "button-color": "blue",
            "dialog_option": "greetings",
            "is_enabled": false,
            "num_items": 5
        });
        let new_default = json!({
            "button-color": "green",
            "new_homepage": true,
            "is_enabled": true,
            "num_items": 10,
            "item_order": ["first", "second", "third"],
        });
        let merged = merge_two_defaults(&old_default, &new_default);
        assert_eq!(
            json!({
                "button-color": "green",
                "dialog_option": "greetings",
                "is_enabled": true,
                "num_items": 10,
                "new_homepage": true,
                "item_order": ["first", "second", "third"],
            }),
            merged
        );
        Ok(())
    }

    #[test]
    fn test_merge_two_defaults_map_intersection_recursive_merge() -> Result<()> {
        // if they intersect on both key and type, but the type intersected is not an object, we just take the new one
        let old_default = json!({
            "button-color": "blue",
            "dialog_item": {
                "title": "hello",
                "message": "bobo",
                "priority": 10,
            },
            "is_enabled": false,
            "num_items": 5
        });
        let new_default = json!({
            "button-color": "green",
            "new_homepage": true,
            "is_enabled": true,
            "dialog_item": {
                "message": "fofo",
                "priority": 11,
                "subtitle": "hey there"
            },
            "num_items": 10,
            "item_order": ["first", "second", "third"],
        });
        let merged = merge_two_defaults(&old_default, &new_default);
        assert_eq!(
            json!({
                "button-color": "green",
                "dialog_item": {
                    "title": "hello",
                    "message": "fofo",
                    "priority": 11,
                    "subtitle": "hey there"
                },
                "is_enabled": true,
                "num_items": 10,
                "new_homepage": true,
                "item_order": ["first", "second", "third"],
            }),
            merged
        );
        Ok(())
    }

    #[test]
    fn test_merge_two_defaults_highlevel_non_maps() -> Result<()> {
        let old_default = json!(["array", "json"]);
        let new_default = json!(["another", "array"]);
        let merged = merge_two_defaults(&old_default, &new_default);
        assert_eq!(json!(["another", "array"]), merged);
        Ok(())
    }

    #[test]
    fn test_channel_defaults_channels_no_merging() -> Result<()> {
        let input: Vec<DefaultBlock> = serde_json::from_value(json!([
            {
                "channel": "release",
                "value": {
                    "button-color": "green"
                }
            },
            {
                "channel": "nightly",
                "value": {
                    "button-color": "dark-green"
                }
            },
            {
                "channel": "beta",
                "value": {
                    "button-color": "light-green"
                }
            }
        ]))?;
        let res = collect_channel_defaults(
            &input,
            &[
                "release".to_string(),
                "nightly".to_string(),
                "beta".to_string(),
            ],
        )?;
        assert_eq!(
            vec![
                (
                    "release".to_string(),
                    json!({
                        "button-color": "green"
                    })
                ),
                (
                    "nightly".to_string(),
                    json!({
                        "button-color": "dark-green"
                    })
                ),
                (
                    "beta".to_string(),
                    json!({
                        "button-color": "light-green"
                    })
                )
            ]
            .into_iter()
            .collect::<HashMap<_, _>>(),
            res
        );
        Ok(())
    }

    #[test]
    fn test_channel_defaults_channels_merging_same_channel() -> Result<()> {
        let input: Vec<DefaultBlock> = serde_json::from_value(json!([
            {
                "channel": "release",
                "value": {
                    "button-color": "green"
                }
            },
            {
                "channel": "nightly",
                "value": {
                    "button-color": "dark-green",
                    "title": "heya"
                }
            },
            {
                "channel": "beta",
                "value": {
                    "button-color": "light-green"
                }
            },
            {
                "channel": "nightly",
                "value": {
                    "button-color": "dark-red",
                    "subtitle": "hello",
                }
            },
            {
                "channel": "beta",
                "value": {
                    "title": "hello there"
                }
            }
        ]))?;
        let res = collect_channel_defaults(
            &input,
            &[
                "release".to_string(),
                "nightly".to_string(),
                "beta".to_string(),
            ],
        )?;
        assert_eq!(
            vec![
                (
                    "release".to_string(),
                    json!({
                        "button-color": "green"
                    })
                ),
                (
                    "nightly".to_string(),
                    json!({
                        "button-color": "dark-red",
                        "title": "heya",
                        "subtitle": "hello"
                    })
                ),
                (
                    "beta".to_string(),
                    json!({
                        "button-color": "light-green",
                        "title": "hello there"
                    })
                )
            ]
            .into_iter()
            .collect::<HashMap<_, _>>(),
            res
        );
        Ok(())
    }

    #[test]
    fn test_channel_defaults_no_channel_applies_to_all() -> Result<()> {
        let input: Vec<DefaultBlock> = serde_json::from_value(json!([
            {
                "channel": "release",
                "value": {
                    "button-color": "green"
                }
            },
            {
                "channel": "nightly",
                "value": {
                    "button-color": "dark-green"
                }
            },
            {
                "channel": "beta",
                "value": {
                    "button-color": "light-green"
                }
            },
            {
                "value": {
                    "title": "heya"
                }
            }
        ]))?;
        let res = collect_channel_defaults(
            &input,
            &[
                "release".to_string(),
                "nightly".to_string(),
                "beta".to_string(),
            ],
        )?;
        assert_eq!(
            vec![
                (
                    "release".to_string(),
                    json!({
                        "button-color": "green",
                        "title": "heya"
                    })
                ),
                (
                    "nightly".to_string(),
                    json!({
                        "button-color": "dark-green",
                        "title": "heya"
                    })
                ),
                (
                    "beta".to_string(),
                    json!({
                        "button-color": "light-green",
                        "title": "heya"
                    })
                )
            ]
            .into_iter()
            .collect::<HashMap<_, _>>(),
            res
        );
        Ok(())
    }

    #[test]
    fn test_channel_defaults_no_channel_overwrites_all() -> Result<()> {
        let input: Vec<DefaultBlock> = serde_json::from_value(json!([
            {
                "channel": "release",
                "value": {
                    "button-color": "green"
                }
            },
            {
                "channel": "nightly",
                "value": {
                    "button-color": "dark-green"
                }
            },
            {
                "channel": "beta",
                "value": {
                    "button-color": "light-green"
                }
            },
            {
                "value": {
                    "button-color": "red"
                }
            }
        ]))?;
        let res = collect_channel_defaults(
            &input,
            &[
                "release".to_string(),
                "nightly".to_string(),
                "beta".to_string(),
            ],
        )?;
        assert_eq!(
            vec![
                (
                    "release".to_string(),
                    json!({
                        "button-color": "red"
                    })
                ),
                (
                    "nightly".to_string(),
                    json!({
                        "button-color": "red"
                    })
                ),
                (
                    "beta".to_string(),
                    json!({
                        "button-color": "red"
                    })
                )
            ]
            .into_iter()
            .collect::<HashMap<_, _>>(),
            res
        );
        Ok(())
    }

    #[test]
    fn test_channel_defaults_no_channel_gets_overwritten_if_followed_by_channel() -> Result<()> {
        let input: Vec<DefaultBlock> = serde_json::from_value(json!([
            {
                "channel": "release",
                "value": {
                    "button-color": "green"
                }
            },
            {
                "channel": "nightly",
                "value": {
                    "button-color": "dark-green"
                }
            },
            {
                "channel": "beta",
                "value": {
                    "button-color": "light-green"
                }
            },
            {
                "value": {
                    "button-color": "red"
                }
            },
            {
                "channel": "nightly",
                "value": {
                    "button-color": "dark-red"
                }
            }
        ]))?;
        let res = collect_channel_defaults(
            &input,
            &[
                "release".to_string(),
                "nightly".to_string(),
                "beta".to_string(),
            ],
        )?;
        assert_eq!(
            vec![
                (
                    "release".to_string(),
                    json!({
                        "button-color": "red"
                    })
                ),
                (
                    "nightly".to_string(),
                    json!({
                        "button-color": "dark-red"
                    })
                ),
                (
                    "beta".to_string(),
                    json!({
                        "button-color": "red"
                    })
                )
            ]
            .into_iter()
            .collect::<HashMap<_, _>>(),
            res
        );
        Ok(())
    }

    #[test]
    fn test_channel_defaults_channels_multiple() -> Result<()> {
        let input: Vec<DefaultBlock> = serde_json::from_value(json!([
            {
                "channels": ["release", "beta"],
                "value": {
                    "button-color": "green"
                }
            },
        ]))?;
        let res = collect_channel_defaults(&input, &["release".to_string(), "beta".to_string()])?;
        assert_eq!(
            vec![
                (
                    "release".to_string(),
                    json!({
                        "button-color": "green"
                    })
                ),
                (
                    "beta".to_string(),
                    json!({
                        "button-color": "green"
                    })
                )
            ]
            .into_iter()
            .collect::<HashMap<_, _>>(),
            res
        );
        Ok(())
    }

    #[test]
    fn test_channel_defaults_channel_multiple_merge_channels_multiple() -> Result<()> {
        let input: Vec<DefaultBlock> = serde_json::from_value(json!([
            {
                "channel": "nightly, debug",
                "channels": ["release", "beta"],
                "value": {
                    "button-color": "green"
                }
            },
        ]))?;
        let res = collect_channel_defaults(
            &input,
            &[
                "release".to_string(),
                "beta".to_string(),
                "nightly".to_string(),
                "debug".to_string(),
            ],
        )?;
        assert_eq!(
            vec![
                (
                    "release".to_string(),
                    json!({
                        "button-color": "green"
                    })
                ),
                (
                    "beta".to_string(),
                    json!({
                        "button-color": "green"
                    })
                ),
                (
                    "nightly".to_string(),
                    json!({
                        "button-color": "green"
                    })
                ),
                (
                    "debug".to_string(),
                    json!({
                        "button-color": "green"
                    })
                )
            ]
            .into_iter()
            .collect::<HashMap<_, _>>(),
            res
        );
        Ok(())
    }

    #[test]
    fn test_channel_defaults_fail_if_invalid_channel_supplied() -> Result<()> {
        let input: Vec<DefaultBlock> = serde_json::from_value(json!([
            {
                "channel": "release",
                "value": {
                    "button-color": "green"
                }
            },
            {
                "channel": "nightly",
                "value": {
                    "button-color": "dark-green"
                }
            },
            {
                "channel": "beta",
                "value": {
                    "button-color": "light-green"
                }
            },
            {
                "channel": "bobo",
                "value": {
                    "button-color": "no color"
                }
            }
        ]))?;
        let res = collect_channel_defaults(
            &input,
            &[
                "release".to_string(),
                "nightly".to_string(),
                "beta".to_string(),
            ],
        )
        .expect_err("Should return error");
        if let FMLError::InvalidChannelError(channel, _supported) = res {
            assert!(channel.contains("bobo"));
        } else {
            panic!(
                "Should have returned a InvalidChannelError, returned {:?}",
                res
            )
        }
        Ok(())
    }

    #[test]
    fn test_channel_defaults_empty_default_created_if_none_supplied_in_feature() -> Result<()> {
        let input: Vec<DefaultBlock> = serde_json::from_value(json!([
            {
                "channel": "release",
                "value": {
                    "button-color": "green"
                }
            },
            {
                "channel": "nightly",
                "value": {
                    "button-color": "dark-green"
                }
            },
            // No entry fo beta supplied, we will still get an entry in the result
            // but it will be empty
        ]))?;
        let res = collect_channel_defaults(
            &input,
            &[
                "release".to_string(),
                "nightly".to_string(),
                "beta".to_string(),
            ],
        )?;
        assert_eq!(
            vec![
                (
                    "release".to_string(),
                    json!({
                        "button-color": "green"
                    })
                ),
                (
                    "nightly".to_string(),
                    json!({
                        "button-color": "dark-green"
                    })
                ),
                ("beta".to_string(), json!({}))
            ]
            .into_iter()
            .collect::<HashMap<_, _>>(),
            res
        );
        Ok(())
    }

    #[test]
    fn test_merge_feature_default_unsupported_channel() -> Result<()> {
        let mut feature_def: FeatureDef = Default::default();
        let merger = DefaultsMerger::new(
            Default::default(),
            vec!["release".into(), "beta".into()],
            "nightly".into(),
        );
        let err = merger
            .merge_feature_defaults(&mut feature_def, &None)
            .expect_err("Should return an error");
        if let FMLError::InvalidChannelError(channel, _supported) = err {
            assert!(channel.contains("nightly"));
        } else {
            panic!(
                "Should have returned an InvalidChannelError, returned: {:?}",
                err
            );
        }
        Ok(())
    }

    #[test]
    fn test_merge_feature_default_overwrite_field_default_based_on_channel() -> Result<()> {
        let mut feature_def = FeatureDef {
            props: vec![PropDef {
                name: "button-color".into(),
                default: json!("blue"),
                doc: "".into(),
                typ: TypeRef::String,
            }],
            ..Default::default()
        };
        let default_blocks = serde_json::from_value(json!([
            {
                "channel": "nightly",
                "value": {
                    "button-color": "dark-green"
                }
            },
            {
                "channel": "release",
                "value": {
                    "button-color": "green"
                }
            },
            {
                "channel": "beta",
                "value": {
                    "button-color": "light-green"
                }
            },
        ]))?;
        let merger = DefaultsMerger::new(
            Default::default(),
            vec!["release".into(), "beta".into(), "nightly".into()],
            "nightly".into(),
        );
        merger.merge_feature_defaults(&mut feature_def, &default_blocks)?;
        assert_eq!(
            feature_def.props,
            vec![PropDef {
                name: "button-color".into(),
                default: json!("dark-green"),
                doc: "".into(),
                typ: TypeRef::String,
            }]
        );
        Ok(())
    }

    #[test]
    fn test_merge_feature_default_field_default_not_overwritten_if_no_feature_default_for_channel(
    ) -> Result<()> {
        let mut feature_def = FeatureDef {
            props: vec![PropDef {
                name: "button-color".into(),
                default: json!("blue"),
                doc: "".into(),
                typ: TypeRef::String,
            }],
            ..Default::default()
        };
        let default_blocks = serde_json::from_value(json!([{
            "channel": "release",
            "value": {
                "button-color": "green"
            }
        },
        {
            "channel": "beta",
            "value": {
                "button-color": "light-green"
            }
        }]))?;
        let merger = DefaultsMerger::new(
            Default::default(),
            vec!["release".into(), "beta".into(), "nightly".into()],
            "nightly".into(),
        );
        merger.merge_feature_defaults(&mut feature_def, &default_blocks)?;
        assert_eq!(
            feature_def.props,
            vec![PropDef {
                name: "button-color".into(),
                default: json!("blue"),
                doc: "".into(),
                typ: TypeRef::String,
            }]
        );
        Ok(())
    }

    #[test]
    fn test_merge_feature_default_overwrite_nested_field_default() -> Result<()> {
        let mut feature_def = FeatureDef {
            props: vec![PropDef {
                name: "Dialog".into(),
                default: json!({
                    "button-color": "blue",
                    "title": "hello",
                    "inner": {
                        "bobo": "fofo",
                        "other-field": "other-value"
                    }
                }),
                doc: "".into(),
                typ: TypeRef::String,
            }],

            ..Default::default()
        };
        let default_blocks = serde_json::from_value(json!([
            {
                "channel": "nightly",
                "value": {
                    "Dialog": {
                        "button-color": "dark-green",
                        "inner": {
                            "bobo": "nightly"
                        }
                    }
                }
            },
            {
                "channel": "release",
                "value": {
                    "Dialog": {
                        "button-color": "green",
                        "inner": {
                            "bobo": "release",
                            "new-field": "new-value"
                        }
                    }
                }
            },
            {
                "channel": "beta",
                "value": {
                    "Dialog": {
                        "button-color": "light-green",
                        "inner": {
                            "bobo": "beta"
                        }
                    }
                }
            },
        ]))?;
        let merger = DefaultsMerger::new(
            Default::default(),
            vec!["release".into(), "beta".into(), "nightly".into()],
            "release".into(),
        );
        merger.merge_feature_defaults(&mut feature_def, &default_blocks)?;
        assert_eq!(
            feature_def.props,
            vec![PropDef {
                name: "Dialog".into(),
                default: json!({
                        "button-color": "green",
                        "title": "hello",
                        "inner": {
                            "bobo": "release",
                            "other-field": "other-value",
                            "new-field": "new-value"
                        }
                }),
                doc: "".into(),
                typ: TypeRef::String,
            }]
        );
        Ok(())
    }

    #[test]
    fn test_merge_feature_default_overwrite_field_default_based_on_channel_using_only_no_channel_default(
    ) -> Result<()> {
        let mut feature_def = FeatureDef {
            props: vec![PropDef {
                name: "button-color".into(),
                default: json!("blue"),
                doc: "".into(),
                typ: TypeRef::String,
            }],
            ..Default::default()
        };
        let default_blocks = serde_json::from_value(json!([
            // No channel applies to all channel
            // so the nightly channel will get this
            {
                "value": {
                    "button-color": "dark-green"
                }
            },
            {
                "channel": "release",
                "value": {
                    "button-color": "green"
                }
            },
            {
                "channel": "beta",
                "value": {
                    "button-color": "light-green"
                }
            },
        ]))?;
        let merger = DefaultsMerger::new(
            Default::default(),
            vec!["release".into(), "beta".into(), "nightly".into()],
            "nightly".into(),
        );
        merger.merge_feature_defaults(&mut feature_def, &default_blocks)?;
        assert_eq!(
            feature_def.props,
            vec![PropDef {
                name: "button-color".into(),
                default: json!("dark-green"),
                doc: "".into(),
                typ: TypeRef::String,
            }]
        );
        Ok(())
    }

    #[test]
    fn test_include_check_can_merge_manifest() -> Result<()> {
        let files = FileLoader::default()?;
        let parser = Parser::new(files, std::env::temp_dir().as_path().into())?;
        let parent_path: FilePath = std::env::temp_dir().as_path().into();
        let child_path = parent_path.join("http://not-needed.com")?;
        let parent = ManifestFrontEnd {
            channels: vec!["alice".to_string(), "bob".to_string()],
            ..Default::default()
        };
        let child = ManifestFrontEnd {
            channels: vec!["alice".to_string(), "bob".to_string()],
            ..Default::default()
        };

        assert!(parser
            .check_can_merge_manifest(&parent_path, &parent, &child_path, &child)
            .is_ok());

        let child = ManifestFrontEnd {
            channels: vec!["eve".to_string()],
            ..Default::default()
        };

        assert!(parser
            .check_can_merge_manifest(&parent_path, &parent, &child_path, &child)
            .is_err());

        Ok(())
    }

    #[test]
    fn test_include_check_can_merge_manifest_with_imports() -> Result<()> {
        let files = FileLoader::default()?;
        let parser = Parser::new(files, std::env::temp_dir().as_path().into())?;
        let parent_path: FilePath = std::env::temp_dir().as_path().into();
        let child_path = parent_path.join("http://child")?;
        let parent = ManifestFrontEnd {
            channels: vec!["alice".to_string(), "bob".to_string()],
            imports: vec![ImportBlock {
                path: "absolute_path".to_string(),
                channel: "one_channel".to_string(),
                features: Default::default(),
            }],
            ..Default::default()
        };
        let child = ManifestFrontEnd {
            channels: vec!["alice".to_string(), "bob".to_string()],
            imports: vec![ImportBlock {
                path: "absolute_path".to_string(),
                channel: "another_channel".to_string(),
                features: Default::default(),
            }],
            ..Default::default()
        };

        let mut map = Default::default();
        let res = parser.check_can_merge_imports(&parent_path, &parent.imports, &mut map);
        assert!(res.is_ok());
        assert_eq!(map.get("absolute_path").unwrap(), "one_channel");

        let err_msg = "Problem with http://child/: File absolute_path is imported with two different channels: one_channel and another_channel";
        let res = parser.check_can_merge_imports(&child_path, &child.imports, &mut map);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().to_string(), err_msg.to_string());

        let res = parser.check_can_merge_manifest(&parent_path, &parent, &child_path, &child);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().to_string(), err_msg.to_string());

        Ok(())
    }

    #[test]
    fn test_include_circular_includes() -> Result<()> {
        use crate::util::pkg_dir;
        // snake.yaml includes tail.yaml, which includes snake.yaml
        let path = PathBuf::from(pkg_dir()).join("fixtures/fe/including/circular/snake.yaml");

        let files = FileLoader::default()?;
        let parser = Parser::new(files, path.as_path().into())?;
        let ir = parser.get_intermediate_representation("release");
        assert!(ir.is_ok());

        Ok(())
    }

    #[test]
    fn test_include_deeply_nested_includes() -> Result<()> {
        use crate::util::pkg_dir;
        // Deeply nested includes, which start at 00-head.yaml, and then recursively includes all the
        // way down to 06-toe.yaml
        let path_buf = PathBuf::from(pkg_dir()).join("fixtures/fe/including/deep/00-head.yaml");

        let files = FileLoader::default()?;
        let parser = Parser::new(files, path_buf.as_path().into())?;

        let ir = parser.get_intermediate_representation("release")?;
        assert_eq!(ir.feature_defs.len(), 1);

        Ok(())
    }
}
