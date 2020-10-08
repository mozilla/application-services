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
#[cfg(debug_assertions)]
pub use evaluator::filter_enrolled;

use ::uuid::Uuid;
pub use config::Config;
use http_client::{Client, SettingsClient};
pub use matcher::AppContext;
use persistence::Database;
use serde_derive::*;
use std::path::Path;

const DEFAULT_TOTAL_BUCKETS: u32 = 10000;
const DB_KEY_NIMBUS_ID: &str = "nimbus-id";

/// Nimbus is the main struct representing the experiments state
/// It should hold all the information needed to communicate a specific user's
/// experimentation status
pub struct NimbusClient {
    experiments: Vec<Experiment>,
    enrolled_experiments: Vec<EnrolledExperiment>,
    app_context: AppContext,
    db: Database,
    nimbus_id: Uuid,
}

#[derive(Debug, Clone)]
pub struct EnrolledExperiment {
    pub slug: String,
    pub user_facing_name: String,
    pub user_facing_description: String,
    pub branch_slug: String,
}

impl NimbusClient {
    pub fn new<P: AsRef<Path>>(
        collection_name: String,
        app_context: AppContext,
        db_path: P,
        config: Option<Config>,
        available_randomization_units: AvailableRandomizationUnits,
    ) -> Result<Self> {
        let client = Client::new(&collection_name, config.clone())?;
        let resp = client.get_experiments()?;
        let db = Database::new(db_path)?;
        let nimbus_id = Self::get_or_create_nimbus_id(&db)?;
        let enrolled_experiments =
            evaluator::filter_enrolled(&nimbus_id, &available_randomization_units, &resp)?;
        Ok(Self {
            experiments: resp,
            enrolled_experiments,
            app_context,
            db,
            nimbus_id,
        })
    }

    pub fn get_experiment_branch(&self, slug: String) -> Option<String> {
        self.enrolled_experiments
            .iter()
            .find(|e| e.slug == slug)
            .map(|e| e.branch_slug.clone())
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

    pub fn nimbus_id(&self) -> Uuid {
        self.nimbus_id
    }

    fn get_or_create_nimbus_id(db: &Database) -> Result<Uuid> {
        Ok(match db.get(DB_KEY_NIMBUS_ID)? {
            Some(nimbus_id) => nimbus_id,
            None => {
                let nimbus_id = Uuid::new_v4();
                db.put(DB_KEY_NIMBUS_ID, &nimbus_id)?;
                nimbus_id
            }
        })
    }
}

#[derive(Deserialize, Serialize, Debug, Default, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Experiment {
    pub slug: String,
    pub application: String,
    pub user_facing_name: String,
    pub user_facing_description: String,
    pub is_enrollment_paused: bool,
    pub bucket_config: BucketConfig,
    pub probe_sets: Vec<String>,
    pub branches: Vec<Branch>,
    pub targeting: Option<String>,
    pub start_date: Option<String>, // TODO: Use a date format here
    pub end_date: Option<String>,   // TODO: Use a date format here
    pub proposed_duration: Option<u32>,
    pub proposed_enrollment: u32,
    pub reference_branch: Option<String>,
    // N.B. eecords in RemoteSettings will have `id` and `filter_expression` fields,
    // but we ignore them because they're for internal use by RemoteSettings.
}

#[derive(Deserialize, Serialize, Debug, Default, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FeatureConfig {
    pub feature_id: String,
    pub enabled: bool,
    // There is a nullable `value` field that can contain key-value config options
    // that modify the behaviour of an application feature, but we don't support
    // it yet and the details are still being finalized, so we ignore it for now.
}

#[derive(Deserialize, Serialize, Debug, Default, Clone, PartialEq)]
pub struct Branch {
    pub slug: String,
    pub ratio: u32,
    pub feature: Option<FeatureConfig>,
}

fn default_buckets() -> u32 {
    DEFAULT_TOTAL_BUCKETS
}

#[derive(Deserialize, Serialize, Debug, Default, Clone, PartialEq)]
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
    NimbusId,
    ClientId,
}

impl Default for RandomizationUnit {
    fn default() -> Self {
        Self::NimbusId
    }
}

pub struct AvailableRandomizationUnits {
    pub client_id: Option<String>,
}

impl AvailableRandomizationUnits {
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

include!(concat!(env!("OUT_DIR"), "/nimbus.uniffi.rs"));
