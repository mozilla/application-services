use std::fmt::Display;

use serde::Serialize;
use serde_json::json;

use crate::intermediate_representation::{PropDef, TypeRef};
use crate::{
    intermediate_representation::FeatureManifest, Config, GenerateExperimenterManifestCmd,
};

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::Result;

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExperimenterFeatureManifest {
    description: String,
    has_exposure: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    exposure_description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_early_startup: Option<bool>,
    // Not happy for us to use [`serde_json::Value`] but
    // the variables definition includes arbitrary keys
    variables: Variables,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct Variables(serde_json::Value);

impl From<FeatureManifest> for ExperimenterFeatureManifest {
    fn from(fm: FeatureManifest) -> Self {
        // TODO: Figure out how experimenter consumes features
        // i.e, does it expect each feature individually, or all together
        // for the sake of a draft, I just generate the first one
        let feature = fm.feature_defs[0].clone();
        Self {
            description: feature.doc(),
            has_exposure: true,
            is_early_startup: None,
            // NOTE: Where does this description come from
            // and should it be in our IR for the FML
            exposure_description: Some("".into()),
            variables: feature.props().into(),
        }
    }
}

enum ExperimentManifestPropType {
    Json,
    Boolean,
    Int,
    String,
}

impl Display for ExperimentManifestPropType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ExperimentManifestPropType::Boolean => "boolean",
            ExperimentManifestPropType::Int => "int",
            ExperimentManifestPropType::Json => "json",
            ExperimentManifestPropType::String => "string",
        };
        write!(f, "{}", s)
    }
}

impl From<TypeRef> for ExperimentManifestPropType {
    fn from(typ: TypeRef) -> Self {
        match typ {
            TypeRef::Object(_)
            | TypeRef::EnumMap(_, _)
            | TypeRef::StringMap(_)
            | TypeRef::List(_)
            | TypeRef::Enum(_) => Self::Json,
            TypeRef::Boolean => Self::Boolean,
            TypeRef::Int => Self::Int,
            // TODO: are the bundle types equivalent to strings for experimenter purposes?
            TypeRef::String | TypeRef::BundleImage(_) | TypeRef::BundleText(_) => Self::String,
            TypeRef::Option(inner) => Self::from(inner),
        }
    }
}

impl From<Box<TypeRef>> for ExperimentManifestPropType {
    fn from(typ: Box<TypeRef>) -> Self {
        (*typ).into()
    }
}

impl From<Vec<PropDef>> for Variables {
    fn from(properties: Vec<PropDef>) -> Self {
        let mut map = serde_json::Map::new();
        properties.iter().for_each(|prop| {
            let typ = ExperimentManifestPropType::from(prop.typ()).to_string();
            map.insert(
                prop.name(),
                json!({
                    "type": typ,
                    "description": prop.doc(),
                }),
            );
        });
        Self(serde_json::Value::Object(map))
    }
}

pub(crate) fn generate_manifest(
    ir: FeatureManifest,
    _config: Config,
    cmd: GenerateExperimenterManifestCmd,
) -> Result<()> {
    let experiment_manifest: ExperimenterFeatureManifest = ir.into();
    let output_str = serde_json::to_string_pretty(&experiment_manifest)?;
    std::fs::write(cmd.output, output_str)?;
    Ok(())
}
