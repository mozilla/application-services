/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::collections::{BTreeMap, HashMap, HashSet};

use email_address::EmailAddress;
use serde::{Deserialize, Serialize};
use serde_json::json;
use url::Url;

use crate::{
    defaults::DefaultsMerger,
    error::Result,
    intermediate_representation::{
        EnumDef, FeatureDef, FeatureManifest, ModuleId, ObjectDef, PropDef, TargetLanguage,
        TypeRef, VariantDef,
    },
    parser::get_typeref_from_string,
};

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub(crate) struct EnumVariantBody {
    pub(crate) description: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub(crate) struct EnumBody {
    pub(crate) description: String,
    pub(crate) variants: BTreeMap<String, EnumVariantBody>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct FeatureFieldBody {
    #[serde(flatten)]
    pub(crate) field: FieldBody,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) pref_key: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub(crate) struct FieldBody {
    pub(crate) description: String,
    #[serde(rename = "type")]
    pub(crate) variable_type: String,
    pub(crate) default: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub(crate) struct ObjectBody {
    pub(crate) description: String,
    // We need these in a deterministic order, so they are stable across multiple
    // runs of the same manifests.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) fields: BTreeMap<String, FieldBody>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct Types {
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) enums: BTreeMap<String, EnumBody>,
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) objects: BTreeMap<String, ObjectBody>,
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

    #[allow(unused)]
    pub(crate) fn supports(&self, lang: &TargetLanguage) -> bool {
        match lang {
            TargetLanguage::Kotlin => self.kotlin_about.is_some(),
            TargetLanguage::Swift => self.swift_about.is_some(),
            TargetLanguage::IR => true,
            TargetLanguage::ExperimenterYAML => true,
            TargetLanguage::ExperimenterJSON => true,
        }
    }

    #[allow(unused)]
    pub fn description_only(&self) -> Self {
        AboutBlock {
            description: self.description.clone(),
            kotlin_about: None,
            swift_about: None,
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
    pub(crate) features: BTreeMap<String, Vec<DefaultBlock>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct FeatureBody {
    #[serde(flatten)]
    pub(crate) metadata: FeatureMetadata,
    // We need these in a deterministic order, so they are stable across multiple
    // runs of the same manifests:
    // 1. Swift insists on args in the same order they were declared.
    // 2. imported features are declared and constructed in different runs of the tool.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) variables: BTreeMap<String, FeatureFieldBody>,
    #[serde(alias = "defaults")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) default: Option<Vec<DefaultBlock>>,
    #[serde(default)]
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) allow_coenrollment: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct FeatureMetadata {
    pub(crate) description: String,
    /// A list of named URLs to documentation for this feature.
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) documentation: Vec<DocumentationLink>,
    /// A list of contacts (engineers, product owners) who can be contacted for
    /// help with this feature. Specifically for QA questions.
    #[serde(default)]
    #[serde(alias = "owners", alias = "owner")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) contacts: Vec<EmailAddress>,
    /// Where should QA file issues for this feature?
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) meta_bug: Option<Url>,
    /// What Glean events can the feature produce?
    /// These should be links to a Glean dictionary.
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) events: Vec<Url>,
    /// A link to a Web based configuration UI for this feature.
    /// This UI should produce the valid JSON instead of typing it
    /// by hand.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) configurator: Option<Url>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct DocumentationLink {
    pub(crate) name: String,
    pub(crate) url: Url,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestFrontEnd {
    #[serde(default)]
    pub(crate) version: String,
    #[serde(default)]
    pub(crate) about: Option<AboutBlock>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) channels: Vec<String>,

    #[serde(default)]
    #[serde(alias = "include")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) includes: Vec<String>,

    #[serde(default)]
    #[serde(alias = "import")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) imports: Vec<ImportBlock>,

    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) features: BTreeMap<String, FeatureBody>,

    // We'd like to get rid of the `types` property,
    // but we need to keep supporting it.
    #[serde(default)]
    #[serde(rename = "types")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) legacy_types: Option<Types>,

    // If a types attribute isn't explicitly expressed,
    // then we should assume that we use the flattened version.
    #[serde(default)]
    #[serde(flatten)]
    pub(crate) types: Types,
}

impl ManifestFrontEnd {
    pub fn channels(&self) -> Vec<String> {
        self.channels.clone()
    }

    pub fn includes(&self) -> Vec<String> {
        self.includes.clone()
    }

    /// Retrieves all the types represented in the Manifest
    ///
    /// # Returns
    /// Returns a [`std::collections::HashMap<String,TypeRef>`] where
    /// the key is the name of the type, and the TypeRef represents the type itself
    fn get_types(&self) -> HashMap<String, TypeRef> {
        let types = self.legacy_types.as_ref().unwrap_or(&self.types);
        types
            .enums
            .keys()
            .map(|s| (s.clone(), TypeRef::Enum(s.clone())))
            .chain(
                types
                    .objects
                    .keys()
                    .map(|s| (s.clone(), TypeRef::Object(s.clone()))),
            )
            .collect()
    }

    fn get_prop_def_from_feature_field(&self, nm: &str, body: &FeatureFieldBody) -> PropDef {
        let mut prop = self.get_prop_def_from_field(nm, &body.field);
        prop.pref_key = body.pref_key.clone();
        prop
    }

    /// Transforms a front-end field definition, a tuple of [`String`] and [`FieldBody`],
    /// into a [`PropDef`]
    ///
    /// # Arguments
    /// - `field`: The [`(&String, &FieldBody)`] tuple to get the propdef from
    ///
    /// # Returns
    /// return the IR [`PropDef`]
    fn get_prop_def_from_field(&self, nm: &str, body: &FieldBody) -> PropDef {
        let types = self.get_types();
        PropDef {
            name: nm.into(),
            doc: body.description.clone(),
            typ: match get_typeref_from_string(body.variable_type.to_owned(), &types) {
                Ok(type_ref) => type_ref,
                Err(e) => {
                    // Try matching against the user defined types
                    match types.get(&body.variable_type) {
                        Some(type_ref) => type_ref.to_owned(),
                        None => panic!(
                            "{}\n{} is not a valid FML type or user defined type",
                            e, body.variable_type
                        ),
                    }
                }
            },
            default: json!(body.default),
            pref_key: None,
        }
    }

    /// Retrieves all the feature definitions represented in the manifest
    ///
    /// # Returns
    /// Returns a [`std::collections::BTreeMap<String, FeatureDef>`]
    fn get_feature_defs(&self, merger: &DefaultsMerger) -> Result<BTreeMap<String, FeatureDef>> {
        let mut features: BTreeMap<_, _> = Default::default();
        for (nm, body) in &self.features {
            let mut fields: Vec<_> = Default::default();
            for (fnm, field) in &body.variables {
                fields.push(self.get_prop_def_from_feature_field(fnm, field));
            }
            let mut def = FeatureDef {
                name: nm.clone(),
                metadata: body.metadata.clone(),
                props: fields,
                allow_coenrollment: body.allow_coenrollment,
            };
            merger.merge_feature_defaults(&mut def, &body.default)?;
            features.insert(nm.to_owned(), def);
        }
        Ok(features)
    }

    /// Retrieves all the Object type definitions represented in the manifest
    ///
    /// # Returns
    /// Returns a [`std::collections::BTreeMap<String. ObjectDef>`]
    fn get_objects(&self) -> BTreeMap<String, ObjectDef> {
        let types = self.legacy_types.as_ref().unwrap_or(&self.types);
        let mut objs: BTreeMap<_, _> = Default::default();
        for (nm, body) in &types.objects {
            let mut fields: Vec<_> = Default::default();
            for (fnm, field) in &body.fields {
                fields.push(self.get_prop_def_from_field(fnm, field));
            }
            objs.insert(
                nm.to_owned(),
                ObjectDef {
                    name: nm.clone(),
                    doc: body.description.clone(),
                    props: fields,
                },
            );
        }
        objs
    }

    /// Retrieves all the Enum type definitions represented in the manifest
    ///
    /// # Returns
    /// Returns a [`std::collections::BTreeMap<String, EnumDef>`]
    fn get_enums(&self) -> BTreeMap<String, EnumDef> {
        let types = self.legacy_types.as_ref().unwrap_or(&self.types);
        let mut enums: BTreeMap<_, _> = Default::default();
        for (name, body) in &types.enums {
            let mut variants: Vec<_> = Default::default();
            for (v_name, v_body) in &body.variants {
                variants.push(VariantDef {
                    name: v_name.clone(),
                    doc: v_body.description.clone(),
                });
            }
            enums.insert(
                name.to_owned(),
                EnumDef {
                    name: name.clone(),
                    doc: body.description.clone(),
                    variants,
                },
            );
        }
        enums
    }

    pub(crate) fn get_intermediate_representation(
        &self,
        id: &ModuleId,
        channel: Option<&str>,
    ) -> Result<FeatureManifest> {
        let enums = self.get_enums();
        let objects = self.get_objects();
        let merger =
            DefaultsMerger::new(&objects, self.channels.clone(), channel.map(str::to_string));

        let features = self.get_feature_defs(&merger)?;

        let about = match &self.about {
            Some(a) => a.clone(),
            None => Default::default(),
        };

        Ok(FeatureManifest::new(
            id.clone(),
            channel,
            features,
            enums,
            objects,
            about,
        ))
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DefaultBlock {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) channel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) channels: Option<Vec<String>>,
    pub(crate) value: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) targeting: Option<String>,
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

impl From<serde_json::Value> for DefaultBlock {
    fn from(value: serde_json::Value) -> Self {
        Self {
            value,
            channels: None,
            channel: None,
            targeting: None,
        }
    }
}

#[cfg(test)]
mod about_block {
    use super::*;

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
        assert!(input.merge_channels().is_none())
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
        assert!(res.is_some());
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
        assert!(res.is_some());
        let res = res.unwrap();
        assert!(res.contains(&"a".to_string()));
        assert!(res.len() == 1)
    }
}

#[cfg(test)]
mod feature_metadata {
    use super::*;
    use std::str::FromStr;

    impl DocumentationLink {
        fn new(nm: &str, url: &str) -> Result<Self> {
            Ok(Self {
                name: nm.to_string(),
                url: Url::from_str(url)?,
            })
        }
    }

    #[test]
    fn test_happy_path() -> Result<()> {
        let fm = serde_json::from_str::<FeatureMetadata>(
            r#"{
            "description": "A description",
            "meta-bug": "https://example.com/EXP-23",
            "contacts": [
                "jdoe@example.com"
            ],
            "documentation": [
                {
                    "name": "User documentation",
                    "url": "https://example.info/my-feature"
                }
            ],
            "events": [
                "https://example.com/glean/dictionary/button-pressed"
            ],
            "configurator": "https://auth.example.com/my-feature/configuration-ui"
        }"#,
        )?;
        assert_eq!(
            fm,
            FeatureMetadata {
                description: "A description".to_string(),
                meta_bug: Some(Url::from_str("https://example.com/EXP-23")?),
                contacts: vec![EmailAddress::from_str("jdoe@example.com")?],
                documentation: vec![DocumentationLink::new(
                    "User documentation",
                    "https://example.info/my-feature"
                )?],
                configurator: Some(Url::from_str(
                    "https://auth.example.com/my-feature/configuration-ui"
                )?),
                events: vec![Url::from_str(
                    "https://example.com/glean/dictionary/button-pressed"
                )?,],
            }
        );

        let fm = serde_json::from_str::<FeatureMetadata>(
            r#"{
            "description": "A description",
            "meta-bug": "https://example.com/EXP-23",
            "documentation": [
                {
                    "name": "User documentation",
                    "url": "https://example.info/my-feature"
                }
            ]
        }"#,
        )?;
        assert_eq!(
            fm,
            FeatureMetadata {
                description: "A description".to_string(),
                meta_bug: Some(Url::from_str("https://example.com/EXP-23")?),
                contacts: Default::default(),
                documentation: vec![DocumentationLink::new(
                    "User documentation",
                    "https://example.info/my-feature"
                )?],
                ..Default::default()
            }
        );

        let fm = serde_json::from_str::<FeatureMetadata>(
            r#"{
            "description": "A description",
            "contacts": [
                "jdoe@example.com"
            ]
        }"#,
        )?;
        assert_eq!(
            fm,
            FeatureMetadata {
                description: "A description".to_string(),
                contacts: vec![EmailAddress::from_str("jdoe@example.com")?],
                ..Default::default()
            }
        );

        Ok(())
    }

    #[test]
    fn test_invalid_email_addresses() -> Result<()> {
        let fm = serde_json::from_str::<FeatureMetadata>(
            r#"{
            "description": "A description",
            "contacts": [
                "Not an email address"
            ],
        }"#,
        );
        assert!(fm.is_err());
        Ok(())
    }

    #[test]
    fn test_invalid_urls() -> Result<()> {
        let fm = serde_json::from_str::<FeatureMetadata>(
            r#"{
            "description": "A description",
            "documentation": [
                "Not a url"
            ],
        }"#,
        );
        assert!(fm.is_err());

        let fm = serde_json::from_str::<FeatureMetadata>(
            r#"{
            "description": "A description",
            "meta-bug": "Not a url"
        }"#,
        );
        assert!(fm.is_err());
        Ok(())
    }
}
