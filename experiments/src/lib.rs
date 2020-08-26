// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

pub mod error;
mod evaluator;
pub use error::{Error, Result};
mod config;
mod http_client;
mod matcher;
mod persistence;
mod sampling;
mod uuid;
#[cfg(debug_assertions)]
pub use evaluator::filter_enrolled;

use ::uuid::Uuid;
pub use config::Config as ExperimentConfig;
use http_client::{Client, SettingsClient};
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
    enrolled_experiments: Vec<EnrolledExperiment>,
    app_context: AppContext,
    uuid: Uuid,
}

#[derive(Debug, Clone)]
pub struct EnrolledExperiment {
    pub slug: String,
    pub user_facing_name: String,
    pub user_facing_description: String,
    pub branch_slug: String,
}

impl Experiments {
    pub fn new<P: AsRef<Path>>(
        collection_name: String,
        app_context: AppContext,
        _db_path: P,
        config: Option<ExperimentConfig>,
    ) -> Result<Self> {
        let client = Client::new(&collection_name, config.clone())?;
        let resp = client.get_experiments()?;
        let uuid = uuid::generate_uuid(config);
        log::info!("uuid is {}", uuid);
        let enrolled_experiments = evaluator::filter_enrolled(&uuid, &resp)?;
        Ok(Self {
            experiments: resp,
            enrolled_experiments,
            app_context,
            uuid,
        })
    }

    pub fn get_experiment_branch(&self, _slug: String) -> Option<String> {
        unimplemented!();
    }

    pub fn get_active_experiments(&self) -> Vec<EnrolledExperiment> {
        self.enrolled_experiments.clone()
    }

    pub fn get_all_experiments(&self) -> Vec<Experiment> {
        self.experiments.clone()
    }

    pub fn opt_in_with_branch(&self, _experiment_slug: String, _branch: String) {
        unimplemented!()
    }

    pub fn opt_out(&self, _experiment_slug: String) {
        unimplemented!()
    }

    pub fn opt_out_all(&self) {
        unimplemented!()
    }

    pub fn update_experiments(&self) -> Result<()> {
        unimplemented!()
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
    pub start_date: String,       // TODO: Use a format here
    pub end_date: Option<String>, // TODO: Use a date format here
    // TODO: This shouldn't be optional based on the nimbus schema, but it is for now till the servers are updated
    pub proposed_duration: Option<u32>,
    pub proposed_enrollment: u32,
    pub reference_branch: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub struct Branch {
    pub slug: String,
    pub ratio: u32,
    pub group: Option<Vec<Group>>,
    pub value: Option<BranchValue>,
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

include!(concat!(env!("OUT_DIR"), "/nimbus.uniffi.rs"));
