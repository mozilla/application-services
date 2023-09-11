/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */
#![allow(unused)]

use crate::error::ClientError::{
    InvalidFeatureConfig, InvalidFeatureId, InvalidFeatureValue, JsonMergeError,
};
use crate::error::FMLError::ClientError;
use crate::intermediate_representation::FeatureDef;
use crate::{
    error::{FMLError, Result},
    intermediate_representation::{FeatureManifest, TypeRef},
    parser::Parser,
    util::loaders::FileLoader,
};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

pub struct MergedJsonWithErrors {
    pub json: String,
    pub errors: Vec<FMLError>,
}

pub struct FmlClient {
    pub(crate) manifest: Arc<FeatureManifest>,
    pub(crate) default_json: serde_json::Map<String, serde_json::Value>,
}

fn get_default_json_for_manifest(
    manifest: &FeatureManifest,
) -> Result<serde_json::Map<String, serde_json::Value>> {
    Ok(manifest
        .default_json()
        .as_object()
        .ok_or(ClientError(JsonMergeError(
            "Manifest default json is not an object".to_string(),
        )))?
        .to_owned())
}

impl FmlClient {
    /// Constructs a new FmlClient object.
    ///
    /// Definitions of the parameters are as follows:
    /// - `manifest_path`: The path (relative to the current working directory) to the fml.yml that should be loaded.
    /// - `channel`: The channel that should be loaded for the manifest.
    pub fn new(manifest_path: String, channel: String) -> Result<Self> {
        let files = FileLoader::new(
            std::env::current_dir().expect("Current Working Directory is not set"),
            None,
            Default::default(),
        )?;
        let path = files.file_path(&manifest_path)?;
        let parser: Parser = Parser::new(files, path)?;
        let ir = parser.get_intermediate_representation(&channel)?;
        ir.validate_manifest();

        Ok(FmlClient {
            default_json: get_default_json_for_manifest(&ir)?,
            manifest: Arc::new(ir),
        })
    }

    /// Validates a supplied feature configuration. Returns true or an FMLError.
    pub fn is_feature_valid(&self, feature_id: String, value: JsonObject) -> Result<bool> {
        self.manifest
            .validate_feature_config(&feature_id, serde_json::Value::Object(value))
            .map(|_| true)
    }

    /// Validates a supplied list of feature configurations. The valid configurations will be merged into the manifest's
    /// default feature JSON, and invalid configurations will be returned as a list of their respective errors.
    pub fn merge(
        &self,
        feature_configs: HashMap<String, JsonObject>,
    ) -> Result<MergedJsonWithErrors> {
        let mut json = self.default_json.clone();
        let mut errors: Vec<FMLError> = Default::default();
        for (feature_id, value) in feature_configs {
            match self
                .manifest
                .validate_feature_config(&feature_id, serde_json::Value::Object(value))
            {
                Ok(fd) => {
                    json.insert(feature_id, fd.default_json());
                }
                Err(e) => errors.push(e),
            };
        }
        Ok(MergedJsonWithErrors {
            json: serde_json::to_string(&json)?,
            errors,
        })
    }

    /// Returns the default feature JSON for the loaded FML's selected channel.
    pub fn get_default_json(&self) -> Result<String> {
        Ok(serde_json::to_string(&self.default_json)?)
    }

    /// Returns a list of feature ids that support coenrollment.
    pub fn get_coenrolling_feature_ids(&self) -> Result<Vec<String>> {
        Ok(self.manifest.get_coenrolling_feature_ids())
    }

    pub fn get_feature_ids(&self) -> Vec<String> {
        let mut res: BTreeSet<String> = Default::default();
        for (_, f) in self.manifest.iter_all_feature_defs() {
            res.insert(f.name());
        }
        res.into_iter().collect()
    }

    pub fn get_feature_descriptor(&self, id: String) -> Option<FmlFeatureDescriptor> {
        let (_, f) = self.manifest.find_feature(&id)?;
        Some(f.into())
    }

    pub fn get_feature_descriptors(&self) -> Vec<FmlFeatureDescriptor> {
        let mut res: Vec<_> = Default::default();
        for (_, f) in self.manifest.iter_all_feature_defs() {
            res.push(f.into());
        }
        res
    }
}

#[derive(Debug, PartialEq)]
pub struct FmlFeatureDescriptor {
    id: String,
    description: String,
    is_coenrolling: bool,
}

impl From<&FeatureDef> for FmlFeatureDescriptor {
    fn from(f: &FeatureDef) -> Self {
        Self {
            id: f.name(),
            description: f.doc(),
            is_coenrolling: f.allow_coenrollment,
        }
    }
}

type JsonObject = serde_json::Map<String, serde_json::Value>;

#[cfg(feature = "uniffi-bindings")]
impl UniffiCustomTypeConverter for JsonObject {
    type Builtin = String;

    fn into_custom(val: Self::Builtin) -> uniffi::Result<Self> {
        let json: serde_json::Value = serde_json::from_str(&val)?;

        match json.as_object() {
            Some(obj) => Ok(obj.to_owned()),
            _ => Err(uniffi::deps::anyhow::anyhow!(
                "Unexpected JSON-non-object in the bagging area"
            )),
        }
    }

    fn from_custom(obj: Self) -> Self::Builtin {
        serde_json::Value::Object(obj).to_string()
    }
}

#[cfg(feature = "uniffi-bindings")]
uniffi::include_scaffolding!("fml");

#[cfg(test)]
mod unit_tests {
    use super::*;
    use crate::intermediate_representation::{
        unit_tests::get_feature_manifest, FeatureDef, ModuleId, PropDef,
    };
    use serde_json::{json, Map, Number, Value};
    use std::collections::HashMap;

    impl From<FeatureManifest> for FmlClient {
        fn from(manifest: FeatureManifest) -> Self {
            manifest.validate_manifest().ok();
            FmlClient {
                default_json: get_default_json_for_manifest(&manifest).ok().unwrap(),
                manifest: Arc::new(manifest),
            }
        }
    }

    fn create_manifest() -> FeatureManifest {
        let fm_i = get_feature_manifest(
            vec![],
            vec![],
            vec![FeatureDef {
                name: "feature_i".into(),
                props: vec![PropDef {
                    name: "prop_i_1".into(),
                    typ: TypeRef::String,
                    default: Value::String("prop_i_1_value".into()),
                    doc: "".into(),
                }],
                doc: "feature_i description".to_string(),
                ..Default::default()
            }],
            HashMap::new(),
        );

        get_feature_manifest(
            vec![],
            vec![],
            vec![FeatureDef {
                name: "feature".into(),
                props: vec![PropDef {
                    name: "prop_1".into(),
                    typ: TypeRef::String,
                    default: Value::String("prop_1_value".into()),
                    doc: "".into(),
                }],
                doc: "feature description".to_string(),
                allow_coenrollment: true,
            }],
            HashMap::from([(ModuleId::Local("test".into()), fm_i)]),
        )
    }

    #[test]
    fn test_get_default_json() -> Result<()> {
        let json_result = get_default_json_for_manifest(&create_manifest())?;

        assert_eq!(
            Value::Object(json_result),
            json!({
                "feature": {
                    "prop_1": "prop_1_value"
                },
                "feature_i": {
                    "prop_i_1": "prop_i_1_value"
                }
            })
        );

        Ok(())
    }

    #[test]
    fn test_validate_feature_config() -> Result<()> {
        let client: FmlClient = create_manifest().into();

        assert!(client.is_feature_valid(
            "feature".to_string(),
            Map::from_iter([("prop_1".to_string(), Value::String("new value".into()))])
        )?);

        Ok(())
    }

    #[test]
    fn test_validate_and_merge_feature_configs() -> Result<()> {
        let client: FmlClient = create_manifest().into();

        let result = client.merge(HashMap::from_iter([
            (
                "feature".to_string(),
                Map::from_iter([("prop_1".to_string(), Value::String("new value".to_string()))]),
            ),
            (
                "feature_i".to_string(),
                Map::from_iter([("prop_i_1".to_string(), Value::Number(Number::from(1)))]),
            ),
        ]))?;

        assert_eq!(
            serde_json::from_str::<Value>(&result.json)?,
            json!({
                "feature": {
                    "prop_1": "new value"
                },
                "feature_i": {
                    "prop_i_1": "prop_i_1_value"
                }
            })
        );
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].to_string(), "Validation Error at features/feature_i.prop_i_1: Mismatch between type String and default 1".to_string());

        Ok(())
    }

    #[test]
    fn test_get_coenrolling_feature_ids() -> Result<()> {
        let client: FmlClient = create_manifest().into();
        let result = client.get_coenrolling_feature_ids();

        assert_eq!(result.unwrap(), vec!["feature"]);

        Ok(())
    }

    #[test]
    fn test_feature_ids() -> Result<()> {
        let client: FmlClient = create_manifest().into();
        let result = client.get_feature_ids();

        assert_eq!(result, vec!["feature", "feature_i"]);
        Ok(())
    }

    #[test]
    fn test_get_feature() -> Result<()> {
        let client: FmlClient = create_manifest().into();

        let result = client.get_feature_descriptor("feature".to_string());
        assert!(result.is_some());
        assert_eq!(
            result.unwrap(),
            FmlFeatureDescriptor {
                id: "feature".to_string(),
                description: "feature description".to_string(),
                is_coenrolling: true
            }
        );

        let result = client.get_feature_descriptor("feature_i".to_string());
        assert!(result.is_some());
        assert_eq!(
            result.unwrap(),
            FmlFeatureDescriptor {
                id: "feature_i".to_string(),
                description: "feature_i description".to_string(),
                is_coenrolling: false
            }
        );

        Ok(())
    }
}
