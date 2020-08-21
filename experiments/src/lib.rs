// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

pub mod error;
mod evaluator;
pub use error::*;
mod config;
mod http_client;
mod matcher;
mod persistence;
mod sampling;
mod uuid;

use ::uuid::Uuid;
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
pub use config::Config;
pub use matcher::AppContext;
use serde_derive::*;
use std::path::Path;

const DEFAULT_TOTAL_BUCKETS: u32 = 10000;

/// Experiments is the main struct representing the experiements state
/// It should hold all the information needed to communcate a specific user's
/// Experiementation status
#[derive(Debug, Clone)]
pub struct Experiments {
    experiments: Vec<Experiment>,
    app_context: AppContext,
    uuid: Uuid,
}

impl Experiments {
    pub fn new<P: AsRef<Path>>(
        app_context: AppContext,
        _db_path: P,
        config: Option<Config>,
    ) -> Self {
        let resp = vec![];
        let uuid = uuid::generate_uuid(config);
        Self {
            experiments: resp,
            app_context,
            uuid,
        }
    }

    pub fn get_experiment_branch(&self) -> Result<String> {
        Err(anyhow!("Not implemented"))
    }

    pub fn get_experiments(&self) -> &Vec<Experiment> {
        &self.experiments
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub struct Experiment {
    pub id: String,
    pub filter_expression: String,
    pub targeting: Option<String>,
    pub enabled: bool,
    pub arguments: ExperimentArguments,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentArguments {
    pub slug: String,
    pub user_facing_name: String,
    pub user_facing_description: String,
    pub active: bool,
    pub is_enrollment_paused: bool,
    pub bucket_config: BucketConfig,
    pub features: Vec<String>,
    pub branches: Vec<Branch>,
    pub start_date: DateTime<Utc>,
    pub end_date: Option<DateTime<Utc>>,
    pub proposed_duration: u32,
    pub proposed_enrollment: u32,
    pub reference_branch: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub struct Branch {
    pub slug: String,
    pub ratio: u32,
    pub group: Option<Vec<Group>>,
    pub value: BranchValue,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Group {
    Cfr,
    AboutWelcome,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BranchValue {} // TODO: This is not defined explicitly in the nimbus schema yet

fn default_buckets() -> u32 {
    DEFAULT_TOTAL_BUCKETS
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BucketConfig {
    pub randomization_unit: RandomizationUnit,
    pub namespace: String,
    pub start: u32,
    pub count: u32,
    #[serde(default = "default_buckets")]
    pub total: u32,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RandomizationUnit {
    ClientId,
    NormandyId,
    #[serde(rename = "userId")]
    UserId,
}
