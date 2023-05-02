// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::{defaults::Defaults, enrollment::ExperimentMetadata, NimbusError, Result};
use serde_derive::*;
use serde_json::{from_str, from_value, Map, Value};
use std::collections::HashSet;

const DEFAULT_TOTAL_BUCKETS: u32 = 10000;

#[derive(Debug, Clone)]
pub struct EnrolledExperiment {
    pub feature_ids: Vec<String>,
    pub slug: String,
    pub user_facing_name: String,
    pub user_facing_description: String,
    pub branch_slug: String,
    pub enrollment_id: String,
}

// ⚠️ Attention : Changes to this type should be accompanied by a new test  ⚠️
// ⚠️ in `test_lib_bw_compat.rs`, and may require a DB migration. ⚠️
#[derive(Deserialize, Serialize, Debug, Default, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Experiment {
    pub schema_version: String,
    pub slug: String,
    pub app_name: Option<String>,
    pub app_id: Option<String>,
    pub channel: Option<String>,
    pub user_facing_name: String,
    pub user_facing_description: String,
    pub is_enrollment_paused: bool,
    pub bucket_config: BucketConfig,
    pub branches: Vec<Branch>,
    // The `feature_ids` field was added later. For compatibility with exising experiments
    // and to avoid a db migration, we default it to an empty list when it is missing.
    #[serde(default)]
    pub feature_ids: Vec<String>,
    pub targeting: Option<String>,
    pub start_date: Option<String>, // TODO: Use a date format here
    pub end_date: Option<String>,   // TODO: Use a date format here
    pub proposed_duration: Option<u32>,
    pub proposed_enrollment: u32,
    pub reference_branch: Option<String>,
    #[serde(default)]
    pub is_rollout: bool,
    // N.B. records in RemoteSettings will have `id` and `filter_expression` fields,
    // but we ignore them because they're for internal use by RemoteSettings.
}

#[cfg_attr(not(feature = "stateful"), allow(unused))]
impl Experiment {
    pub(crate) fn has_branch(&self, branch_slug: &str) -> bool {
        self.branches
            .iter()
            .any(|branch| branch.slug == branch_slug)
    }

    pub(crate) fn get_branch(&self, branch_slug: &str) -> Option<&Branch> {
        self.branches.iter().find(|b| b.slug == branch_slug)
    }

    pub(crate) fn get_feature_ids(&self) -> Vec<String> {
        let branches = &self.branches;
        let feature_ids = branches
            .iter()
            .flat_map(|b| {
                b.get_feature_configs()
                    .iter()
                    .map(|f| f.to_owned().feature_id)
                    .collect::<Vec<_>>()
            })
            .collect::<HashSet<_>>();

        feature_ids.into_iter().collect()
    }
}

impl ExperimentMetadata for Experiment {
    fn get_slug(&self) -> String {
        self.slug.clone()
    }

    fn is_rollout(&self) -> bool {
        self.is_rollout
    }
}

pub fn parse_experiments(payload: &str) -> Result<Vec<Experiment>> {
    // We first encode the response into a `serde_json::Value`
    // to allow us to deserialize each experiment individually,
    // omitting any malformed experiments
    let value: Value = from_str(payload)?;
    let data = value
        .get("data")
        .ok_or(NimbusError::InvalidExperimentFormat)?;
    let mut res = Vec::new();
    for exp in data
        .as_array()
        .ok_or(NimbusError::InvalidExperimentFormat)?
    {
        // XXX: In the future it would be nice if this lived in its own versioned crate so that
        // the schema could be decoupled from the sdk so that it can be iterated on while the
        // sdk depends on a particular version of the schema through the Cargo.toml.
        match from_value::<Experiment>(exp.clone()) {
            Ok(exp) => res.push(exp),
            Err(e) => {
                log::trace!("Malformed experiment data: {:#?}", exp);
                log::warn!(
                    "Malformed experiment found! Experiment {},  Error: {}",
                    exp.get("id").unwrap_or(&serde_json::json!("ID_NOT_FOUND")),
                    e
                );
            }
        }
    }
    Ok(res)
}

#[derive(Deserialize, Serialize, Debug, Default, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FeatureConfig {
    pub feature_id: String,
    // There is a nullable `value` field that can contain key-value config options
    // that modify the behaviour of an application feature. Uniffi doesn't quite support
    // serde_json yet.
    #[serde(default)]
    pub value: Map<String, Value>,
}

impl Defaults for FeatureConfig {
    fn defaults(&self, fallback: &Self) -> Result<Self> {
        if self.feature_id != fallback.feature_id {
            // This is unlikely to happen, but if it does it's a bug in Nimbus
            Err(NimbusError::InternalError(
                "Cannot merge feature configs from different features",
            ))
        } else {
            Ok(FeatureConfig {
                feature_id: self.feature_id.clone(),
                value: self.value.defaults(&fallback.value)?,
            })
        }
    }
}

// ⚠️ Attention : Changes to this type should be accompanied by a new test  ⚠️
// ⚠️ in `test_lib_bw_compat.rs`, and may require a DB migration. ⚠️
#[derive(Deserialize, Serialize, Debug, Default, Clone, PartialEq, Eq)]
pub struct Branch {
    pub slug: String,
    pub ratio: i32,
    // we skip serializing the `feature` and `features`
    // fields if they are `None`, to stay aligned
    // with the schema, where only one of them
    // will exist
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feature: Option<FeatureConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub features: Option<Vec<FeatureConfig>>,
}

impl Branch {
    pub(crate) fn get_feature_configs(&self) -> Vec<FeatureConfig> {
        // Some versions of desktop need both, but features should be prioritized
        // (https://mozilla-hub.atlassian.net/browse/SDK-440).
        match (&self.features, &self.feature) {
            (Some(features), _) => features.clone(),
            (None, Some(feature)) => vec![feature.clone()],
            _ => Default::default(),
        }
    }
}

fn default_buckets() -> u32 {
    DEFAULT_TOTAL_BUCKETS
}

// ⚠️ Attention : Changes to this type should be accompanied by a new test  ⚠️
// ⚠️ in `test_lib_bw_compat.rs`, and may require a DB migration. ⚠️
#[derive(Deserialize, Serialize, Debug, Default, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BucketConfig {
    pub randomization_unit: RandomizationUnit,
    pub namespace: String,
    pub start: u32,
    pub count: u32,
    #[serde(default = "default_buckets")]
    pub total: u32,
}

#[allow(unused)]
#[cfg(test)]
impl BucketConfig {
    pub(crate) fn always() -> Self {
        Self {
            start: 0,
            count: default_buckets(),
            total: default_buckets(),
            ..Default::default()
        }
    }
}

// This type is passed across the FFI to client consumers, e.g. UI for testing tooling.
pub struct AvailableExperiment {
    pub slug: String,
    pub user_facing_name: String,
    pub user_facing_description: String,
    pub branches: Vec<ExperimentBranch>,
    pub reference_branch: Option<String>,
}

pub struct ExperimentBranch {
    pub slug: String,
    pub ratio: i32,
}

impl From<Experiment> for AvailableExperiment {
    fn from(exp: Experiment) -> Self {
        Self {
            slug: exp.slug,
            user_facing_name: exp.user_facing_name,
            user_facing_description: exp.user_facing_description,
            branches: exp.branches.into_iter().map(|b| b.into()).collect(),
            reference_branch: exp.reference_branch,
        }
    }
}

impl From<Branch> for ExperimentBranch {
    fn from(branch: Branch) -> Self {
        Self {
            slug: branch.slug,
            ratio: branch.ratio,
        }
    }
}

// ⚠️ Attention : Changes to this type should be accompanied by a new test  ⚠️
// ⚠️ in `test_lib_bw_compat`, and may require a DB migration. ⚠️
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RandomizationUnit {
    NimbusId,
    ClientId,
}

impl Default for RandomizationUnit {
    fn default() -> Self {
        Self::NimbusId
    }
}

#[derive(Default)]
pub struct AvailableRandomizationUnits {
    pub client_id: Option<String>,
    #[allow(dead_code)]
    pub(crate) dummy: i8, // See comments in nimbus.udl for why this hacky item exists.
}

impl AvailableRandomizationUnits {
    // Use ::with_client_id when you want to specify one, or use
    // Default::default if you don't!
    pub fn with_client_id(client_id: &str) -> Self {
        Self {
            client_id: Some(client_id.to_string()),
            dummy: 0,
        }
    }

    pub fn get_value<'a>(
        &'a self,
        nimbus_id: &'a str,
        wanted: &'a RandomizationUnit,
    ) -> Option<&'a str> {
        match wanted {
            RandomizationUnit::NimbusId => Some(nimbus_id),
            RandomizationUnit::ClientId => self.client_id.as_deref(),
        }
    }
}
