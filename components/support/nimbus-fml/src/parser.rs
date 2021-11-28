/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{collections::HashMap, path::Path};

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    error::FMLError,
    intermediate_representation::{
        EnumDef, FeatureDef, FeatureManifest, ObjectDef, PropDef, TypeRef, VariantDef,
    },
    Config,
};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct EnumVariantBody {
    description: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct EnumBody {
    description: String,
    variants: HashMap<String, EnumVariantBody>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct ObjectFieldBody {
    description: String,
    #[serde(default)]
    required: bool,
    #[serde(rename = "type")]
    variable_type: String,
    default: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct ObjectBody {
    description: String,
    failable: Option<bool>,
    fields: HashMap<String, ObjectFieldBody>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct Types {
    enums: HashMap<String, EnumBody>,
    objects: HashMap<String, ObjectBody>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct FeatureVariableBody {
    description: String,
    #[serde(rename = "type")]
    variable_type: String,
    default: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct FeatureBody {
    description: String,
    variables: HashMap<String, FeatureVariableBody>,
    default: Option<serde_json::Value>,
}
#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct ManifestFrontEnd {
    types: Types,
    features: HashMap<String, FeatureBody>,
    channels: Vec<String>,
}

impl ManifestFrontEnd {
    /// Retrieves all the types represented in the Manifest
    ///
    /// # Returns
    /// Returns a [`std::collections::HashMap<String,TypeRef>`] where
    /// the key is the name of the type, and the TypeRef represents the type itself
    fn get_types(&self) -> HashMap<String, TypeRef> {
        self.types
            .enums
            .iter()
            .map(|(s, _)| (s.clone(), TypeRef::Enum(s.clone())))
            .chain(
                self.types
                    .objects
                    .iter()
                    .map(|(s, _)| (s.clone(), TypeRef::Object(s.clone()))),
            )
            .collect()
    }

    /// Retrieves all the feature definitions represented in the manifest
    ///
    /// # Returns
    /// Returns a [`std::vec::Vec<FeatureDef>`]
    fn get_feature_defs(&self) -> Vec<FeatureDef> {
        let types = self.get_types();
        self.features
            .iter()
            .map(|f| {
                FeatureDef {
                    name: f.0.clone(),
                    doc: f.1.description.clone(),
                    props: f
                        .1
                        .variables
                        .iter()
                        .map(|v| PropDef {
                            name: v.0.clone(),
                            doc: v.1.description.clone(),
                            typ: match get_typeref_from_string(
                                v.1.variable_type.to_owned(),
                                Some(types.clone()),
                            ) {
                                Ok(type_ref) => type_ref,
                                Err(e) => {
                                    // Try matching against the user defined types
                                    match types.get(&v.1.variable_type) {
                                        Some(type_ref) => type_ref.to_owned(),
                                        None => panic!(
                                            "{}\n{} is not a valid FML type or user defined type",
                                            e, v.1.variable_type
                                        ),
                                    }
                                }
                            },
                            default: json!(v.1.default),
                        })
                        .collect(),
                    default: f.1.default.clone(),
                }
            })
            .collect()
    }

    /// Retrieves all the Object type definitions represented in the manifest
    ///
    /// # Returns
    /// Returns a [`std::vec::Vec<ObjectDef>`]
    fn get_objects(&self) -> Vec<ObjectDef> {
        let types = self.get_types();
        self.types
            .objects
            .iter()
            .map(|t| ObjectDef {
                name: t.0.clone(),
                doc: t.1.description.clone(),
                props: t
                    .1
                    .fields
                    .iter()
                    .map(|v| PropDef {
                        name: v.0.clone(),
                        doc: v.1.description.clone(),
                        typ: get_typeref_from_string(
                            v.1.variable_type.clone(),
                            Some(types.clone()),
                        )
                        .unwrap(),
                        default: match v.1.default.clone() {
                            Some(d) => json!(d),
                            None => serde_json::Value::Null,
                        },
                    })
                    .collect(),
            })
            .collect()
    }

    /// Retrieves all the Enum type definitions represented in the manifest
    ///
    /// # Returns
    /// Returns a [`std::vec::Vec<EnumDef>`]
    fn get_enums(&self) -> Vec<EnumDef> {
        self.types
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
}

fn parse_typeref_string(input: String) -> Result<(String, Option<String>), FMLError> {
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
fn collect_channel_defaults(
    defaults: serde_json::Value,
) -> Result<HashMap<String, serde_json::Value>, FMLError> {
    let mut channel_map = HashMap::new();
    let defaults = serde_json::from_value::<Vec<FeatureDefaults>>(defaults)?;
    for default in defaults {
        if let Some(channel) = default.channel {
            if channel_map.contains_key(&channel) && default.targeting.is_none() {
                let old_default = channel_map.get(&channel).unwrap();
                let merged = merge_two_defaults(old_default, &default.value);
                channel_map.insert(channel, merged);
            } else {
                channel_map.insert(channel, default.value);
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
/// If the `channel` is `None` we default to using the `release` channel
///
/// # Returns
/// Returns a transformed [`FeatureDef`] with its defaults merged
fn merge_feature_defaults(
    feature_def: FeatureDef,
    channel: Option<&String>,
) -> Result<FeatureDef, FMLError> {
    let mut variable_defaults = serde_json::Map::new();
    let mut res = feature_def.clone();
    for prop in feature_def.props {
        variable_defaults.insert(prop.name, prop.default);
    }
    if let Some(defaults) = feature_def.default {
        let merged_defaults = collect_channel_defaults(defaults)?;
        // we default to using the `release` channel if no channel was given
        let release_channel = "release".to_string();
        let channel = channel.unwrap_or(&release_channel);
        let default_to_merged = merged_defaults
            .get(channel)
            .ok_or_else(|| FMLError::InvalidChannelError("channel".into()))?;
        let merged = merge_two_defaults(
            &serde_json::Value::Object(variable_defaults),
            default_to_merged,
        );
        let map = merged.as_object().ok_or(FMLError::InternalError(
            "Map was merged into a different type",
        ))?;
        let new_props = res
            .props
            .iter()
            .map(|prop| {
                let mut res = prop.clone();
                if map.contains_key(&prop.name) {
                    res.default = map.get(&prop.name).unwrap().clone();
                }
                res
            })
            .collect::<Vec<_>>();

        res.props = new_props;
    }
    Ok(res)
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
                if merged.contains_key(&key) {
                    let old_val = merged.get(&key).unwrap().clone();
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
        "BundleText" => Ok(TypeRef::BundleText(type_name.unwrap())),
        "BundleImage" => Ok(TypeRef::BundleImage(type_name.unwrap())),
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
    enums: Vec<EnumDef>,
    objects: Vec<ObjectDef>,
    features: Vec<FeatureDef>,
    channels: Vec<String>,
}

impl Parser {
    pub fn new(config: &Config, path: &Path) -> Result<Parser, FMLError> {
        let manifest = serde_yaml::from_str::<ManifestFrontEnd>(&std::fs::read_to_string(path)?)?;
        let enums = manifest.get_enums();
        let objects = manifest.get_objects();
        let features = manifest
            .get_feature_defs()
            .into_iter()
            .map(|feature_def| merge_feature_defaults(feature_def, config.channel.as_ref()))
            .collect::<Result<Vec<_>, FMLError>>()?;
        Ok(Parser {
            enums,
            objects,
            features,
            channels: manifest.channels,
        })
    }

    pub fn get_intermediate_representation(&self) -> Result<FeatureManifest, FMLError> {
        Ok(FeatureManifest {
            enum_defs: self.enums.clone(),
            obj_defs: self.objects.clone(),
            hints: HashMap::new(),
            feature_defs: self.features.clone(),
        })
    }
}

#[derive(Debug, Deserialize)]
struct FeatureDefaults {
    channel: Option<String>,
    value: serde_json::Value,
    targeting: Option<String>,
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use crate::{
        error::Result,
        util::{join, pkg_dir},
    };

    #[test]
    fn test_parse_from_front_end_representation() -> Result<()> {
        let path = join(pkg_dir(), "fixtures/fe/nimbus_features.yaml");
        let path_buf = Path::new(&path);
        let parser = Parser::new(&Default::default(), path_buf)?;
        let ir = parser.get_intermediate_representation()?;

        // Validate parsed enums
        assert!(ir.enum_defs.len() == 1);
        assert!(ir.enum_defs.contains(parser.enums.first().unwrap()));
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
        assert!(ir.obj_defs.contains(parser.objects.first().unwrap()));
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
        assert!(ir.feature_defs.contains(parser.features.first().unwrap()));
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
                    "child": {},
                    "adult": {}
                })
        );

        Ok(())
    }

    #[test]
    fn test_merging_defaults() -> Result<()> {
        let path = join(pkg_dir(), "fixtures/fe/default_merging.yaml");
        let path_buf = Path::new(&path);
        let parser = Parser::new(&Default::default(), path_buf)?;
        let ir = parser.get_intermediate_representation()?;
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
        let config: Config = Config {
            channel: Some("nightly".into()),
            ..Default::default()
        };
        let parser = Parser::new(&config, path_buf)?;
        let ir = parser.get_intermediate_representation()?;
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
}
