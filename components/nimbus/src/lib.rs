// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod dbcache;
mod enrollment;
pub mod error;
mod evaluator;
use chrono::{DateTime, NaiveDateTime, Utc};
use defaults::Defaults;
pub use error::{NimbusError, Result};
mod client;
mod config;
mod defaults;
mod matcher;
pub mod persistence;
mod sampling;
mod updating;
#[cfg(debug_assertions)]
pub use evaluator::evaluate_enrollment;

use client::{create_client, parse_experiments, SettingsClient};
pub use config::RemoteSettingsConfig;
use dbcache::DatabaseCache;
pub use enrollment::EnrollmentStatus;
use enrollment::{
    get_global_user_participation, opt_in_with_branch, opt_out, set_global_user_participation,
    EnrollmentChangeEvent, EnrollmentsEvolver,
};
use evaluator::is_experiment_available;

// Exposed for Example only
pub use evaluator::TargetingAttributes;

// We only use this in a test, and with --no-default-features, we don't use it
// at all
#[allow(unused_imports)]
use enrollment::EnrollmentChangeEventType;

pub use matcher::AppContext;
use once_cell::sync::OnceCell;
use persistence::{Database, StoreId, Writer};
use serde_derive::*;
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use updating::{read_and_remove_pending_experiments, write_pending_experiments};
use uuid::Uuid;

const DEFAULT_TOTAL_BUCKETS: u32 = 10000;
const DB_KEY_NIMBUS_ID: &str = "nimbus-id";
pub const DB_KEY_INSTALLATION_DATE: &str = "installation-date";
pub const DB_KEY_UPDATE_DATE: &str = "update-date";
pub const DB_KEY_APP_VERSION: &str = "app-version";

impl From<AppContext> for TargetingAttributes {
    fn from(app_context: AppContext) -> Self {
        Self {
            app_context,
            ..Default::default()
        }
    }
}
// The main `NimbusClient` struct must not expose any methods that make an `&mut self`,
// in order to be compatible with the uniffi's requirements on objects. This is a helper
// struct to contain the bits that do actually need to be mutable, so they can be
// protected by a Mutex.
#[derive(Default)]
struct InternalMutableState {
    available_randomization_units: AvailableRandomizationUnits,
    // Application level targeting attributes
    targeting_attributes: TargetingAttributes,
}

/// Nimbus is the main struct representing the experiments state
/// It should hold all the information needed to communicate a specific user's
/// experimentation status
pub struct NimbusClient {
    settings_client: Mutex<Box<dyn SettingsClient + Send>>,
    mutable_state: Mutex<InternalMutableState>,
    app_context: AppContext,
    db: OnceCell<Database>,
    // Manages an in-memory cache so that we can answer certain requests
    // without doing (or waiting for) IO.
    database_cache: DatabaseCache,
    db_path: PathBuf,
}

impl NimbusClient {
    // This constructor *must* not do any kind of I/O since it might be called on the main
    // thread in the gecko Javascript stack, hence the use of OnceCell for the db.
    pub fn new<P: Into<PathBuf>>(
        app_context: AppContext,
        db_path: P,
        config: Option<RemoteSettingsConfig>,
        available_randomization_units: AvailableRandomizationUnits,
    ) -> Result<Self> {
        let settings_client = Mutex::new(create_client(config)?);

        let mutable_state = Mutex::new(InternalMutableState {
            available_randomization_units,
            targeting_attributes: app_context.clone().into(),
        });
        Ok(Self {
            settings_client,
            mutable_state,
            app_context,
            database_cache: Default::default(),
            db_path: db_path.into(),
            db: OnceCell::default(),
        })
    }

    #[cfg(test)]
    pub fn with_targeting_attributes(&mut self, targeting_attributes: TargetingAttributes) {
        let mut state = self.mutable_state.lock().unwrap();
        state.targeting_attributes = targeting_attributes;
    }

    #[cfg(test)]
    pub fn get_targeting_attributes(&self) -> TargetingAttributes {
        let state = self.mutable_state.lock().unwrap();
        state.targeting_attributes.clone()
    }

    pub fn initialize(&self) -> Result<()> {
        let db = self.db()?;
        // We're not actually going to write, we just want to exclude concurrent writers.
        let writer = db.write()?;
        self.database_cache.commit_and_update(db, writer)?;
        Ok(())
    }

    // Note: the contract for this function is that it never blocks on IO.
    pub fn get_experiment_branch(&self, slug: String) -> Result<Option<String>> {
        self.database_cache.get_experiment_branch(&slug)
    }

    pub fn get_feature_config_variables(&self, feature_id: String) -> Result<Option<String>> {
        self.database_cache
            .get_feature_config_variables(&feature_id)
    }

    pub fn get_experiment_branches(&self, slug: String) -> Result<Vec<ExperimentBranch>> {
        self.get_all_experiments()?
            .into_iter()
            .find(|e| e.slug == slug)
            .map(|e| e.branches.into_iter().map(|b| b.into()).collect())
            .ok_or(NimbusError::NoSuchExperiment(slug))
    }

    pub fn get_global_user_participation(&self) -> Result<bool> {
        let db = self.db()?;
        let reader = db.read()?;
        get_global_user_participation(db, &reader)
    }

    pub fn set_global_user_participation(
        &self,
        user_participating: bool,
    ) -> Result<Vec<EnrollmentChangeEvent>> {
        let db = self.db()?;
        let mut writer = db.write()?;
        set_global_user_participation(db, &mut writer, user_participating)?;

        let existing_experiments: Vec<Experiment> =
            db.get_store(StoreId::Experiments).collect_all(&writer)?;
        // We pass the existing experiments as "updated experiments"
        // to the evolver.
        let nimbus_id = self.read_or_create_nimbus_id(db, &mut writer)?;
        let state = self.mutable_state.lock().unwrap();
        let evolver = EnrollmentsEvolver::new(
            &nimbus_id,
            &state.available_randomization_units,
            &state.targeting_attributes,
        );
        let events = evolver.evolve_enrollments_in_db(db, &mut writer, &existing_experiments)?;
        self.database_cache.commit_and_update(db, writer)?;
        Ok(events)
    }

    pub fn get_active_experiments(&self) -> Result<Vec<EnrolledExperiment>> {
        self.database_cache.get_active_experiments()
    }

    pub fn get_all_experiments(&self) -> Result<Vec<Experiment>> {
        let db = self.db()?;
        let reader = db.read()?;
        db.get_store(StoreId::Experiments)
            .collect_all::<Experiment, _>(&reader)
    }

    pub fn get_available_experiments(&self) -> Result<Vec<AvailableExperiment>> {
        Ok(self
            .get_all_experiments()?
            .into_iter()
            .filter(|exp| is_experiment_available(&self.app_context, exp, false))
            .map(|exp| exp.into())
            .collect())
    }

    pub fn opt_in_with_branch(
        &self,
        experiment_slug: String,
        branch: String,
    ) -> Result<Vec<EnrollmentChangeEvent>> {
        let db = self.db()?;
        let mut writer = db.write()?;
        let result = opt_in_with_branch(db, &mut writer, &experiment_slug, &branch)?;
        self.database_cache.commit_and_update(db, writer)?;
        Ok(result)
    }

    pub fn opt_out(&self, experiment_slug: String) -> Result<Vec<EnrollmentChangeEvent>> {
        let db = self.db()?;
        let mut writer = db.write()?;
        let result = opt_out(db, &mut writer, &experiment_slug)?;
        self.database_cache.commit_and_update(db, writer)?;
        Ok(result)
    }

    pub fn update_experiments(&self) -> Result<Vec<EnrollmentChangeEvent>> {
        self.fetch_experiments()?;
        self.apply_pending_experiments()
    }

    pub fn fetch_experiments(&self) -> Result<()> {
        log::info!("fetching experiments");
        let settings_client = self.settings_client.lock().unwrap();
        let new_experiments = settings_client.fetch_experiments()?;
        let db = self.db()?;
        let mut writer = db.write()?;
        write_pending_experiments(db, &mut writer, new_experiments)?;
        writer.commit()?;
        Ok(())
    }

    pub fn apply_pending_experiments(&self) -> Result<Vec<EnrollmentChangeEvent>> {
        log::info!("updating experiment list");
        // If the application did not pass in an installation date,
        // we check if we already persisted one on a previous run:
        let db = self.db()?;
        let mut writer = db.write()?;

        let installation_date = self.get_installation_date(db, &mut writer)?;
        log::info!("[Nimbus] Installation Date: {}", installation_date);
        let update_date = self.get_update_date(db, &mut writer)?;
        log::info!("[Nimbus] Update Date: {}", update_date);
        let now = Utc::now();
        let duration_since_install = now - installation_date;
        log::info!(
            "[Nimbus] Days since install: {}",
            duration_since_install.num_days()
        );
        let duration_since_update = now - update_date;
        log::info!(
            "[Nimbus] Days since update: {}",
            duration_since_update.num_days()
        );
        let mut state = self.mutable_state.lock().unwrap();
        if state.targeting_attributes.days_since_install.is_none() {
            state.targeting_attributes.days_since_install =
                Some(duration_since_install.num_days() as i32);
        }
        if state.targeting_attributes.days_since_update.is_none() {
            state.targeting_attributes.days_since_update =
                Some(duration_since_update.num_days() as i32);
        }

        let pending_updates = read_and_remove_pending_experiments(db, &mut writer)?;
        let res = match pending_updates {
            Some(new_experiments) => {
                let nimbus_id = self.read_or_create_nimbus_id(db, &mut writer)?;
                let evolver = EnrollmentsEvolver::new(
                    &nimbus_id,
                    &state.available_randomization_units,
                    &state.targeting_attributes,
                );
                evolver.evolve_enrollments_in_db(db, &mut writer, &new_experiments)?
            }
            None => vec![],
        };
        self.database_cache.commit_and_update(db, writer)?;
        Ok(res)
    }

    fn get_installation_date(&self, db: &Database, writer: &mut Writer) -> Result<DateTime<Utc>> {
        // we first check our context
        if let Some(context_installation_date) = self.app_context.installation_date {
            let res = DateTime::<Utc>::from_utc(
                NaiveDateTime::from_timestamp(context_installation_date / 1_000, 0),
                Utc,
            );
            log::info!("[Nimbus] Retrieved date from Context: {}", res);
            return Ok(res);
        }
        let store = db.get_store(StoreId::Meta);
        let persisted_installation_date: Option<DateTime<Utc>> =
            store.get(writer, DB_KEY_INSTALLATION_DATE)?;
        Ok(
            if let Some(installation_date) = persisted_installation_date {
                installation_date
            } else if let Some(home_directory) = &self.app_context.home_directory {
                let installation_date = match self.get_creation_date_from_path(home_directory) {
                    Ok(installation_date) => installation_date,
                    Err(e) => {
                        log::warn!("[Nimbus] Unable to get installation date from path, defaulting to today: {:?}", e);
                        Utc::now()
                    }
                };
                let store = db.get_store(StoreId::Meta);
                store.put(writer, DB_KEY_INSTALLATION_DATE, &installation_date)?;
                installation_date
            } else {
                Utc::now()
            },
        )
    }

    fn get_update_date(&self, db: &Database, writer: &mut Writer) -> Result<DateTime<Utc>> {
        let store = db.get_store(StoreId::Meta);

        let persisted_app_version: Option<String> = store.get(writer, DB_KEY_APP_VERSION)?;
        let update_date: Option<DateTime<Utc>> = store.get(writer, DB_KEY_UPDATE_DATE)?;
        Ok(
            match (
                persisted_app_version,
                &self.app_context.app_version,
                update_date,
            ) {
                // The app been run before, but has not just been updated.
                (Some(persisted), Some(current), Some(date)) if persisted == *current => date,
                // The app has been run before, and just been updated.
                (Some(persisted), Some(current), _) if persisted != *current => {
                    let now = Utc::now();
                    store.put(writer, DB_KEY_APP_VERSION, current)?;
                    store.put(writer, DB_KEY_UPDATE_DATE, &now)?;
                    now
                }
                // The app has just been installed
                (None, Some(current), _) => {
                    let now = Utc::now();
                    store.put(writer, DB_KEY_APP_VERSION, current)?;
                    store.put(writer, DB_KEY_UPDATE_DATE, &now)?;
                    now
                }
                // The current version is not available, or the persisted date is not available.
                (_, _, Some(date)) => date,
                // Either way, this doesn't appear to be a good production environment.
                _ => Utc::now(),
            },
        )
    }

    #[cfg(not(test))]
    fn get_creation_date_from_path<P: AsRef<Path>>(&self, path: P) -> Result<DateTime<Utc>> {
        log::info!("[Nimbus] Getting creation date from path");
        let metadata = std::fs::metadata(path)?;
        let system_time_created = metadata.created()?;
        let date_time_created = DateTime::<Utc>::from(system_time_created);
        log::info!(
            "[Nimbus] Creation date retrieved form path successfully: {}",
            date_time_created
        );
        Ok(date_time_created)
    }

    #[cfg(test)]
    fn get_creation_date_from_path<P: AsRef<Path>>(&self, path: P) -> Result<DateTime<Utc>> {
        use std::io::Read;
        let test_path = path.as_ref().with_file_name("test.json");
        let mut file = std::fs::File::open(test_path)?;
        let mut buf = String::new();
        file.read_to_string(&mut buf)?;

        let res = serde_json::from_str::<DateTime<Utc>>(&buf)?;
        Ok(res)
    }

    pub fn set_experiments_locally(&self, experiments_json: String) -> Result<()> {
        let new_experiments = parse_experiments(&experiments_json)?;
        let db = self.db()?;
        let mut writer = db.write()?;
        write_pending_experiments(db, &mut writer, new_experiments)?;
        writer.commit()?;
        Ok(())
    }

    /// Reset internal state in response to application-level telemetry reset.
    ///
    /// When the user resets their telemetry state in the consuming application, we need learn
    /// the new values of any external randomization units, and we need to reset any unique
    /// identifiers used internally by the SDK. If we don't then we risk accidentally tracking
    /// across the telemetry reset, since we could use Nimbus metrics to link their pings from
    /// before and after the reset.
    ///
    pub fn reset_telemetry_identifiers(
        &self,
        new_randomization_units: AvailableRandomizationUnits,
    ) -> Result<Vec<EnrollmentChangeEvent>> {
        let mut events = vec![];
        let db = self.db()?;
        let mut writer = db.write()?;
        // If we have no `nimbus_id` when we can safely assume that there's
        // no other experiment state that needs to be reset.
        let store = db.get_store(StoreId::Meta);
        if store.get::<String, _>(&writer, DB_KEY_NIMBUS_ID)?.is_some() {
            // Each enrollment state includes a unique `enrollment_id` which we need to clear.
            events = enrollment::reset_telemetry_identifiers(&*db, &mut writer)?;
            // The `nimbus_id` itself is a unique identifier.
            // N.B. we do this last, as a signal that all data has been reset.
            store.delete(&mut writer, DB_KEY_NIMBUS_ID)?;
            self.database_cache.commit_and_update(db, writer)?;
        }
        // (No need to commit `writer` if the above check was false, since we didn't change anything)
        let mut state = self.mutable_state.lock().unwrap();
        state.available_randomization_units = new_randomization_units;
        Ok(events)
    }

    pub fn nimbus_id(&self) -> Result<Uuid> {
        let db = self.db()?;
        let mut writer = db.write()?;
        let uuid = self.read_or_create_nimbus_id(db, &mut writer)?;
        // We don't know whether we needed to generate and save the uuid, so
        // we commit just in case - this is hopefully close to a noop in that
        // case!
        writer.commit()?;
        Ok(uuid)
    }

    fn read_or_create_nimbus_id(&self, db: &Database, writer: &mut Writer) -> Result<Uuid> {
        let store = db.get_store(StoreId::Meta);
        Ok(match store.get(writer, DB_KEY_NIMBUS_ID)? {
            Some(nimbus_id) => nimbus_id,
            None => {
                let nimbus_id = Uuid::new_v4();
                store.put(writer, DB_KEY_NIMBUS_ID, &nimbus_id)?;
                nimbus_id
            }
        })
    }

    // Sets the nimbus ID - TEST ONLY - should not be exposed to real clients.
    // (Useful for testing so you can have some control over what experiments
    // are enrolled)
    pub fn set_nimbus_id(&self, uuid: &Uuid) -> Result<()> {
        let db = self.db()?;
        let mut writer = db.write()?;
        db.get_store(StoreId::Meta)
            .put(&mut writer, DB_KEY_NIMBUS_ID, uuid)?;
        writer.commit()?;
        Ok(())
    }

    fn db(&self) -> Result<&Database> {
        self.db.get_or_try_init(|| Database::new(&self.db_path))
    }
}

#[derive(Debug, Clone)]
pub struct EnrolledExperiment {
    pub feature_ids: Vec<String>,
    pub slug: String,
    pub user_facing_name: String,
    pub user_facing_description: String,
    pub branch_slug: String,
    pub enrollment_id: String,
}

/// This is the currently supported major schema version.
pub const SCHEMA_VERSION: u32 = 1;
// XXX: In the future it would be nice if this lived in its own versioned crate so that
// the schema could be decoupled from the sdk so that it can be iterated on while the
// sdk depends on a particular version of the schema through the cargo.toml.

// ⚠️ Attention : Changes to this type should be accompanied by a new test  ⚠️
// ⚠️ in `mod test_schema_bw_compat` below, and may require a DB migration. ⚠️
#[derive(Deserialize, Serialize, Debug, Default, Clone, PartialEq)]
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

impl Experiment {
    fn has_branch(&self, branch_slug: &str) -> bool {
        self.branches
            .iter()
            .any(|branch| branch.slug == branch_slug)
    }

    fn get_branch(&self, branch_slug: &str) -> Option<&Branch> {
        self.branches.iter().find(|b| b.slug == branch_slug)
    }

    fn get_feature_ids(&self) -> Vec<String> {
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

    fn is_rollout(&self) -> bool {
        self.is_rollout
    }
}

#[derive(Deserialize, Serialize, Debug, Default, Clone, PartialEq)]
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
            Err(NimbusError::InternalError(
                "Merging feature config from different branches",
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
// ⚠️ in `mod test_schema_bw_compat` below, and may require a DB migration. ⚠️
#[derive(Deserialize, Serialize, Debug, Default, Clone, PartialEq)]
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
    fn get_feature_configs(&self) -> Vec<FeatureConfig> {
        // There will never be a time when both `feature` and
        // `features` are set
        match (&self.features, &self.feature) {
            (Some(features), None) => features.clone(),
            (None, Some(feature)) => vec![feature.clone()],
            _ => Default::default(),
        }
    }
}

fn default_buckets() -> u32 {
    DEFAULT_TOTAL_BUCKETS
}

// ⚠️ Attention : Changes to this type should be accompanied by a new test  ⚠️
// ⚠️ in `mod test_schema_bw_compat` below, and may require a DB migration. ⚠️
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

#[cfg(test)]
impl BucketConfig {
    fn always() -> Self {
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
// ⚠️ in `mod test_schema_bw_compat` below, and may require a DB migration. ⚠️
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

#[derive(Default)]
pub struct AvailableRandomizationUnits {
    pub client_id: Option<String>,
    #[allow(dead_code)]
    dummy: i8, // See comments in nimbus.udl for why this hacky item exists.
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

#[cfg(feature = "uniffi-bindings")]
include!(concat!(env!("OUT_DIR"), "/nimbus.uniffi.rs"));

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;
    use chrono::Duration;
    use enrollment::{EnrolledReason, EnrollmentStatus, ExperimentEnrollment};
    use serde_json::json;
    use tempdir::TempDir;

    #[test]
    fn test_telemetry_reset() -> Result<()> {
        let mock_client_id = "client-1".to_string();
        let mock_exp_slug = "exp-1".to_string();
        let mock_exp_branch = "branch-1".to_string();

        let tmp_dir = TempDir::new("test_telemetry_reset")?;
        let client = NimbusClient::new(
            AppContext::default(),
            tmp_dir.path(),
            None,
            AvailableRandomizationUnits {
                client_id: Some(mock_client_id.clone()),
                ..AvailableRandomizationUnits::default()
            },
        )?;

        let get_client_id = || {
            client
                .mutable_state
                .lock()
                .unwrap()
                .available_randomization_units
                .client_id
                .clone()
        };

        // Mock being enrolled in a single experiment.
        let db = client.db()?;
        let mut writer = db.write()?;
        db.get_store(StoreId::Experiments).put(
            &mut writer,
            &mock_exp_slug,
            &Experiment {
                slug: mock_exp_slug.clone(),
                ..Experiment::default()
            },
        )?;
        db.get_store(StoreId::Enrollments).put(
            &mut writer,
            &mock_exp_slug,
            &ExperimentEnrollment {
                slug: mock_exp_slug.clone(),
                status: EnrollmentStatus::new_enrolled(EnrolledReason::Qualified, &mock_exp_branch),
            },
        )?;
        writer.commit()?;

        client.initialize()?;

        // Check expected state before resetting telemetry.
        let orig_nimbus_id = client.nimbus_id()?;
        assert_eq!(get_client_id(), Some(mock_client_id));

        let events = client.reset_telemetry_identifiers(AvailableRandomizationUnits::default())?;

        // We should have reset our nimbus_id.
        assert_ne!(orig_nimbus_id, client.nimbus_id()?);

        // We should have updated the randomization units.
        assert_eq!(get_client_id(), None);

        // We should have been disqualified from the enrolled experiment.
        assert_eq!(client.get_experiment_branch(mock_exp_slug)?, None);

        // We should have returned a single event.
        assert_eq!(events.len(), 1);

        Ok(())
    }

    #[test]
    fn test_installation_date() -> Result<()> {
        let mock_client_id = "client-1".to_string();
        let tmp_dir = TempDir::new("test_installation_date")?;
        // Step 1: We first test that the SDK will default to using the
        // value in the app context if it exists
        let three_days_ago = Utc::now() - Duration::days(3);
        let time_stamp = three_days_ago.timestamp_millis();
        let mut app_context = AppContext {
            installation_date: Some(time_stamp),
            home_directory: Some(tmp_dir.path().to_str().unwrap().to_string()),
            ..Default::default()
        };
        let client = NimbusClient::new(
            app_context.clone(),
            tmp_dir.path(),
            None,
            AvailableRandomizationUnits {
                client_id: Some(mock_client_id.clone()),
                ..AvailableRandomizationUnits::default()
            },
        )?;

        client.initialize()?;
        client.apply_pending_experiments()?;

        // We verify that it's three days, ago. Because that's the date
        // passed into the context
        let targeting_attributes = client.get_targeting_attributes();
        assert!(matches!(targeting_attributes.days_since_install, Some(3)));

        // We now clear the persisted storage
        // to make sure we start from a clear state
        let db = client.db()?;
        let mut writer = db.write()?;
        let store = db.get_store(StoreId::Meta);

        store.clear(&mut writer)?;
        writer.commit()?;

        // Step 2: We test that we will fallback to the
        // filesystem, and if that fails we
        // set Today's date.

        // We recreate our client to make sure
        // we wipe any non-persistent memory
        // this time, with a context that does not
        // include the timestamp
        app_context.installation_date = None;
        let client = NimbusClient::new(
            app_context.clone(),
            tmp_dir.path(),
            None,
            AvailableRandomizationUnits {
                client_id: Some(mock_client_id.clone()),
                ..AvailableRandomizationUnits::default()
            },
        )?;
        delete_test_creation_date(tmp_dir.path()).ok();
        // When we check the filesystem, we will fail. We haven't `set_test_creation_date`
        // yet.
        client.initialize()?;
        client.apply_pending_experiments()?;
        // We verify that it's today.
        let targeting_attributes = client.get_targeting_attributes();
        assert!(matches!(targeting_attributes.days_since_install, Some(0)));

        // Step 3: We test that persisted storage takes precedence over
        // checking the filesystem

        // We recreate our client to make sure
        // we wipe any non-persistent memory
        let client = NimbusClient::new(
            app_context.clone(),
            tmp_dir.path(),
            None,
            AvailableRandomizationUnits {
                client_id: Some(mock_client_id.clone()),
                ..AvailableRandomizationUnits::default()
            },
        )?;
        client.initialize()?;
        // We now store a date for days ago in our file system
        // this shouldn't change the installation date for the nimbus client
        // since client already persisted the date seen earlier.
        let four_days_ago = Utc::now() - Duration::days(4);
        set_test_creation_date(four_days_ago, tmp_dir.path())?;
        client.apply_pending_experiments()?;
        let targeting_attributes = client.get_targeting_attributes();
        // We will **STILL** get a 0 `days_since_install` since we persisted the value
        // we got on the previous run, therefore we did not check the file system.
        assert!(matches!(targeting_attributes.days_since_install, Some(0)));

        // We now clear the persisted storage
        // to make sure we start from a clear state
        let db = client.db()?;
        let mut writer = db.write()?;
        let store = db.get_store(StoreId::Meta);

        store.clear(&mut writer)?;
        writer.commit()?;

        // Step 4: We test that if the storage is clear, we will fallback to the
        let client = NimbusClient::new(
            app_context,
            tmp_dir.path(),
            None,
            AvailableRandomizationUnits {
                client_id: Some(mock_client_id),
                ..AvailableRandomizationUnits::default()
            },
        )?;
        client.initialize()?;
        // now that the store is clear, we will fallback again to the
        // file system, and retrieve the four_days_ago number we stored earlier
        client.apply_pending_experiments()?;
        let targeting_attributes = client.get_targeting_attributes();
        assert!(matches!(targeting_attributes.days_since_install, Some(4)));
        Ok(())
    }

    #[test]
    fn test_days_since_update_changes_with_context() -> Result<()> {
        let mock_client_id = "client-1".to_string();
        let tmp_dir = TempDir::new("test_days_since_update")?;
        let client = NimbusClient::new(
            AppContext::default(),
            tmp_dir.path(),
            None,
            AvailableRandomizationUnits {
                client_id: Some(mock_client_id.clone()),
                ..AvailableRandomizationUnits::default()
            },
        )?;
        client.initialize()?;

        // Step 1: Test what happens if we have no persisted app version,
        // but we got a new version in our app_context.
        // We should set our update date to today.

        // We re-create the client, with an app context that includes
        // a version
        let mut app_context = AppContext {
            app_version: Some("v94.0.0".into()),
            ..Default::default()
        };
        let client = NimbusClient::new(
            app_context.clone(),
            tmp_dir.path(),
            None,
            AvailableRandomizationUnits {
                client_id: Some(mock_client_id.clone()),
                ..AvailableRandomizationUnits::default()
            },
        )?;
        client.initialize()?;
        client.apply_pending_experiments()?;
        let targeting_attributes = client.get_targeting_attributes();
        // The days_since_update should be zero
        assert!(matches!(targeting_attributes.days_since_update, Some(0)));
        let db = client.db()?;
        let reader = db.read()?;
        let store = db.get_store(StoreId::Meta);
        let app_version: String = store.get(&reader, DB_KEY_APP_VERSION)?.unwrap();
        // we make sure we persisted the version we saw
        assert_eq!(app_version, "v94.0.0");
        let update_date: DateTime<Utc> = store.get(&reader, DB_KEY_UPDATE_DATE)?.unwrap();
        let diff_with_today = Utc::now() - update_date;
        // we make sure the persisted date, is today
        assert_eq!(diff_with_today.num_days(), 0);

        // Step 2: Test what happens if there is already a persisted date
        // but we get a new one in our context that is the **same**
        // the update_date should not change
        let client = NimbusClient::new(
            app_context.clone(),
            tmp_dir.path(),
            None,
            AvailableRandomizationUnits {
                client_id: Some(mock_client_id.clone()),
                ..AvailableRandomizationUnits::default()
            },
        )?;
        client.initialize()?;
        client.apply_pending_experiments()?;

        // We repeat the same tests we did above first
        let targeting_attributes = client.get_targeting_attributes();
        // The days_since_update should still be zero
        assert!(matches!(targeting_attributes.days_since_update, Some(0)));
        let db = client.db()?;
        let reader = db.read()?;
        let store = db.get_store(StoreId::Meta);
        let app_version: String = store.get(&reader, DB_KEY_APP_VERSION)?.unwrap();
        // we make sure we persisted the version we saw
        assert_eq!(app_version, "v94.0.0");
        let new_update_date: DateTime<Utc> = store.get(&reader, DB_KEY_UPDATE_DATE)?.unwrap();
        // we make sure the persisted date, is **EXACTLY** the same
        // one we persisted earler, not that the `DateTime` object here
        // includes time to the nanoseconds, so this is a valid way
        // to ensure the objects are the same
        assert_eq!(new_update_date, update_date);

        // Step 3: Test what happens if there is a persisted date,
        // but the app_context includes a newer date, the update_date
        // should be updated

        app_context.app_version = Some("v94.0.1".into()); // A different version
        let client = NimbusClient::new(
            app_context,
            tmp_dir.path(),
            None,
            AvailableRandomizationUnits {
                client_id: Some(mock_client_id),
                ..AvailableRandomizationUnits::default()
            },
        )?;
        client.initialize()?;
        client.apply_pending_experiments()?;

        // We repeat some of the same tests we did above first
        let targeting_attributes = client.get_targeting_attributes();
        // The days_since_update should still be zero
        assert!(matches!(targeting_attributes.days_since_update, Some(0)));
        let db = client.db()?;
        let reader = db.read()?;
        let store = db.get_store(StoreId::Meta);
        let app_version: String = store.get(&reader, DB_KEY_APP_VERSION)?.unwrap();
        // we make sure we persisted the **NEW** version we saw
        assert_eq!(app_version, "v94.0.1");
        let new_update_date: DateTime<Utc> = store.get(&reader, DB_KEY_UPDATE_DATE)?.unwrap();
        // we make sure the persisted date is newer and different
        // than the old one. This helps us ensure that there was indeed
        // an update to the date
        assert!(new_update_date > update_date);

        Ok(())
    }

    #[test]
    fn test_days_since_install() -> Result<()> {
        let mock_client_id = "client-1".to_string();

        let temp_dir = TempDir::new("test_days_since_install_failed")?;
        let app_context = AppContext {
            app_name: "fenix".to_string(),
            app_id: "org.mozilla.fenix".to_string(),
            channel: "nightly".to_string(),
            ..Default::default()
        };
        let mut client = NimbusClient::new(
            app_context.clone(),
            temp_dir.path(),
            None,
            AvailableRandomizationUnits {
                client_id: Some(mock_client_id),
                ..AvailableRandomizationUnits::default()
            },
        )?;
        let targeting_attributes = TargetingAttributes {
            app_context,
            days_since_install: Some(10),
            days_since_update: None,
            is_already_enrolled: false,
        };
        client.with_targeting_attributes(targeting_attributes);
        client.initialize()?;
        let experiment_json = serde_json::to_string(&json!({
            "data": [{
                "schemaVersion": "1.0.0",
                "slug": "secure-gold",
                "endDate": null,
                "featureIds": ["some-feature"],
                "branches": [
                    {
                    "slug": "control",
                    "ratio": 1
                    },
                    {
                    "slug": "treatment",
                    "ratio": 1
                    }
                ],
                "channel": "nightly",
                "probeSets": [],
                "startDate": null,
                "appName": "fenix",
                "appId": "org.mozilla.fenix",
                "bucketConfig": {
                    "count": 10000,
                    "start": 0,
                    "total": 10000,
                    "namespace": "secure-gold",
                    "randomizationUnit": "nimbus_id"
                },
                "targeting": "days_since_install == 10",
                "userFacingName": "test experiment",
                "referenceBranch": "control",
                "isEnrollmentPaused": false,
                "proposedEnrollment": 7,
                "userFacingDescription": "This is a test experiment for testing purposes.",
                "id": "secure-copper",
                "last_modified": 1_602_197_324_372i64,
            }
        ]}))?;
        client.set_experiments_locally(experiment_json)?;
        client.apply_pending_experiments()?;

        // The targeting targeted days_since_install == 10, which is true in the client
        // so we should be enrolled in that experiment
        let active_experiments = client.get_active_experiments()?;
        assert_eq!(active_experiments.len(), 1);
        assert_eq!(active_experiments[0].slug, "secure-gold");
        Ok(())
    }

    #[test]
    fn test_days_since_install_failed_targeting() -> Result<()> {
        let mock_client_id = "client-1".to_string();

        let temp_dir = TempDir::new("test_days_since_install_failed")?;
        let app_context = AppContext {
            app_name: "fenix".to_string(),
            app_id: "org.mozilla.fenix".to_string(),
            channel: "nightly".to_string(),
            ..Default::default()
        };
        let mut client = NimbusClient::new(
            app_context.clone(),
            temp_dir.path(),
            None,
            AvailableRandomizationUnits {
                client_id: Some(mock_client_id),
                ..AvailableRandomizationUnits::default()
            },
        )?;
        let targeting_attributes = TargetingAttributes {
            app_context,
            days_since_install: Some(10),
            days_since_update: None,
            is_already_enrolled: false,
        };
        client.with_targeting_attributes(targeting_attributes);
        client.initialize()?;
        let experiment_json = serde_json::to_string(&json!({
            "data": [{
                "schemaVersion": "1.0.0",
                "slug": "secure-gold",
                "endDate": null,
                "featureIds": ["some-feature"],
                "branches": [
                    {
                    "slug": "control",
                    "ratio": 1
                    },
                    {
                    "slug": "treatment",
                    "ratio": 1
                    }
                ],
                "channel": "nightly",
                "probeSets": [],
                "startDate": null,
                "appName": "fenix",
                "appId": "org.mozilla.fenix",
                "bucketConfig": {
                    "count": 10000,
                    "start": 0,
                    "total": 10000,
                    "namespace": "secure-gold",
                    "randomizationUnit": "nimbus_id"
                },
                "targeting": "days_since_install < 10",
                "userFacingName": "test experiment",
                "referenceBranch": "control",
                "isEnrollmentPaused": false,
                "proposedEnrollment": 7,
                "userFacingDescription": "This is a test experiment for testing purposes.",
                "id": "secure-copper",
                "last_modified": 1_602_197_324_372i64,
            }
        ]}))?;
        client.set_experiments_locally(experiment_json)?;
        client.apply_pending_experiments()?;

        // The targeting targeted days_since_install < 10, which is false in the client
        // so we should be enrolled in that experiment
        let active_experiments = client.get_active_experiments()?;
        assert_eq!(active_experiments.len(), 0);
        Ok(())
    }

    #[test]
    fn test_days_since_update() -> Result<()> {
        let mock_client_id = "client-1".to_string();

        let temp_dir = TempDir::new("test_days_since_update")?;
        let app_context = AppContext {
            app_name: "fenix".to_string(),
            app_id: "org.mozilla.fenix".to_string(),
            channel: "nightly".to_string(),
            ..Default::default()
        };
        let mut client = NimbusClient::new(
            app_context.clone(),
            temp_dir.path(),
            None,
            AvailableRandomizationUnits {
                client_id: Some(mock_client_id),
                ..AvailableRandomizationUnits::default()
            },
        )?;
        let targeting_attributes = TargetingAttributes {
            app_context,
            days_since_install: None,
            days_since_update: Some(10),
            is_already_enrolled: false,
        };
        client.with_targeting_attributes(targeting_attributes);
        client.initialize()?;
        let experiment_json = serde_json::to_string(&json!({
            "data": [{
                "schemaVersion": "1.0.0",
                "slug": "secure-gold",
                "endDate": null,
                "featureIds": ["some-feature"],
                "branches": [
                    {
                    "slug": "control",
                    "ratio": 1
                    },
                    {
                    "slug": "treatment",
                    "ratio": 1
                    }
                ],
                "channel": "nightly",
                "probeSets": [],
                "startDate": null,
                "appName": "fenix",
                "appId": "org.mozilla.fenix",
                "bucketConfig": {
                    "count": 10000,
                    "start": 0,
                    "total": 10000,
                    "namespace": "secure-gold",
                    "randomizationUnit": "nimbus_id"
                },
                "targeting": "days_since_update == 10",
                "userFacingName": "test experiment",
                "referenceBranch": "control",
                "isEnrollmentPaused": false,
                "proposedEnrollment": 7,
                "userFacingDescription": "This is a test experiment for testing purposes.",
                "id": "secure-copper",
                "last_modified": 1_602_197_324_372i64,
            }
        ]}))?;
        client.set_experiments_locally(experiment_json)?;
        client.apply_pending_experiments()?;

        // The targeting targeted days_since_update == 10, which is true in the client
        // so we should be enrolled in that experiment
        let active_experiments = client.get_active_experiments()?;
        assert_eq!(active_experiments.len(), 1);
        assert_eq!(active_experiments[0].slug, "secure-gold");
        Ok(())
    }

    #[test]
    fn test_days_since_update_failed_targeting() -> Result<()> {
        let mock_client_id = "client-1".to_string();

        let temp_dir = TempDir::new("test_days_since_update_failed")?;
        let app_context = AppContext {
            app_name: "fenix".to_string(),
            app_id: "org.mozilla.fenix".to_string(),
            channel: "nightly".to_string(),
            ..Default::default()
        };
        let mut client = NimbusClient::new(
            app_context.clone(),
            temp_dir.path(),
            None,
            AvailableRandomizationUnits {
                client_id: Some(mock_client_id),
                ..AvailableRandomizationUnits::default()
            },
        )?;
        let targeting_attributes = TargetingAttributes {
            app_context,
            days_since_install: None,
            days_since_update: Some(10),
            is_already_enrolled: false,
        };
        client.with_targeting_attributes(targeting_attributes);
        client.initialize()?;
        let experiment_json = serde_json::to_string(&json!({
            "data": [{
                "schemaVersion": "1.0.0",
                "slug": "secure-gold",
                "endDate": null,
                "featureIds": ["some-feature"],
                "branches": [
                    {
                    "slug": "control",
                    "ratio": 1
                    },
                    {
                    "slug": "treatment",
                    "ratio": 1
                    }
                ],
                "channel": "nightly",
                "probeSets": [],
                "startDate": null,
                "appName": "fenix",
                "appId": "org.mozilla.fenix",
                "bucketConfig": {
                    "count": 10000,
                    "start": 0,
                    "total": 10000,
                    "namespace": "secure-gold",
                    "randomizationUnit": "nimbus_id"
                },
                "targeting": "days_since_update < 10",
                "userFacingName": "test experiment",
                "referenceBranch": "control",
                "isEnrollmentPaused": false,
                "proposedEnrollment": 7,
                "userFacingDescription": "This is a test experiment for testing purposes.",
                "id": "secure-copper",
                "last_modified": 1_602_197_324_372i64,
            }
        ]}))?;
        client.set_experiments_locally(experiment_json)?;
        client.apply_pending_experiments()?;

        // The targeting targeted days_since_update < 10, which is false in the client
        // so we should be enrolled in that experiment
        let active_experiments = client.get_active_experiments()?;
        assert_eq!(active_experiments.len(), 0);
        Ok(())
    }

    fn set_test_creation_date<P: AsRef<Path>>(date: DateTime<Utc>, path: P) -> Result<()> {
        use std::fs::OpenOptions;
        let test_path = path.as_ref().with_file_name("test.json");
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(test_path)
            .unwrap();
        file.write_all(serde_json::to_string(&date).unwrap().as_bytes())?;
        Ok(())
    }

    fn delete_test_creation_date<P: AsRef<Path>>(path: P) -> Result<()> {
        let test_path = path.as_ref().with_file_name("test.json");
        std::fs::remove_file(test_path)?;
        Ok(())
    }
}

#[cfg(test)]
/// A suite of tests for b/w compat of data storage schema.
///
/// We use the `Serialize/`Deserialize` impls on various structs in order to persist them
/// into rkv, and it's important that we be able to read previously-persisted data even
/// if the struct definitions change over time.
///
/// This is a suite of tests specifically to check for backward compatibility with data
/// that may have been written to disk by previous versions of the library.
///
/// ⚠️ Warning : Do not change the JSON data used by these tests. ⚠️
/// ⚠️ The whole point of the tests is to check things work with that data. ⚠️
///
mod test_schema_bw_compat {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_without_probe_sets_and_enabled() {
        // ⚠️ Warning : Do not change the JSON data used by this test. ⚠️
        // this is an experiment following the schema after the removal
        // of the `enabled` and `probe_sets` fields which were removed
        // together in the same proposal
        serde_json::from_value::<Experiment>(json!({
            "schemaVersion": "1.0.0",
            "slug": "secure-gold",
            "appName": "fenix",
            "appId": "bobo",
            "channel": "nightly",
            "endDate": null,
            "branches":[
                {
                    "slug": "control",
                    "ratio": 1,
                    "features": [{
                        "featureId": "feature1",
                        "value": {
                            "key": "value1"
                        }
                    },
                    {
                        "featureId": "feature2",
                        "value": {
                            "key": "value2"
                        }
                    }]
                },
                {
                    "slug": "treatment",
                    "ratio":1,
                    "features": [{
                        "featureId": "feature3",
                        "value": {
                            "key": "value3"
                        }
                    },
                    {
                        "featureId": "feature4",
                        "value": {
                            "key": "value4"
                        }
                    }]
                }
            ],
            "startDate":null,
            "application":"fenix",
            "bucketConfig":{
                "count":10_000,
                "start":0,
                "total":10_000,
                "namespace":"secure-gold",
                "randomizationUnit":"nimbus_id"
            },
            "userFacingName":"Diagnostic test experiment",
            "referenceBranch":"control",
            "isEnrollmentPaused":false,
            "proposedEnrollment":7,
            "userFacingDescription":"This is a test experiment for diagnostic purposes.",
            "id":"secure-gold",
            "last_modified":1_602_197_324_372i64
        }))
        .unwrap();
    }

    #[test]
    fn test_multifeature_branch_schema() {
        // ⚠️ Warning : Do not change the JSON data used by this test. ⚠️
        // this is an experiment following the schema after the addition
        // of multiple features per branch
        let exp: Experiment = serde_json::from_value(json!({
            "schemaVersion": "1.0.0",
            "slug": "secure-gold",
            "appName": "fenix",
            "appId": "bobo",
            "channel": "nightly",
            "endDate": null,
            "branches":[
                {
                    "slug": "control",
                    "ratio": 1,
                    "features": [{
                        "featureId": "feature1",
                        "enabled": true,
                        "value": {
                            "key": "value1"
                        }
                    },
                    {
                        "featureId": "feature2",
                        "enabled": false,
                        "value": {
                            "key": "value2"
                        }
                    }]
                },
                {
                    "slug": "treatment",
                    "ratio":1,
                    "features": [{
                        "featureId": "feature3",
                        "enabled": true,
                        "value": {
                            "key": "value3"
                        }
                    },
                    {
                        "featureId": "feature4",
                        "enabled": false,
                        "value": {
                            "key": "value4"
                        }
                    }]
                }
            ],
            "probeSets":[],
            "startDate":null,
            "application":"fenix",
            "bucketConfig":{
                "count":10_000,
                "start":0,
                "total":10_000,
                "namespace":"secure-gold",
                "randomizationUnit":"nimbus_id"
            },
            "userFacingName":"Diagnostic test experiment",
            "referenceBranch":"control",
            "isEnrollmentPaused":false,
            "proposedEnrollment":7,
            "userFacingDescription":"This is a test experiment for diagnostic purposes.",
            "id":"secure-gold",
            "last_modified":1_602_197_324_372i64
        }))
        .unwrap();
        assert_eq!(
            exp.branches[0].get_feature_configs(),
            vec![
                FeatureConfig {
                    feature_id: "feature1".to_string(),
                    value: vec![("key".to_string(), json!("value1"))]
                        .into_iter()
                        .collect()
                },
                FeatureConfig {
                    feature_id: "feature2".to_string(),
                    value: vec![("key".to_string(), json!("value2"))]
                        .into_iter()
                        .collect()
                }
            ]
        );
        assert_eq!(
            exp.branches[1].get_feature_configs(),
            vec![
                FeatureConfig {
                    feature_id: "feature3".to_string(),
                    value: vec![("key".to_string(), json!("value3"))]
                        .into_iter()
                        .collect()
                },
                FeatureConfig {
                    feature_id: "feature4".to_string(),
                    value: vec![("key".to_string(), json!("value4"))]
                        .into_iter()
                        .collect()
                }
            ]
        );
        assert!(exp.branches[0].feature.is_none());
        assert!(exp.branches[1].feature.is_none());
    }

    #[test]
    fn test_only_one_feature_branch_schema() {
        // ⚠️ Warning : Do not change the JSON data used by this test. ⚠️
        // this is an experiment following the schema before the addition
        // of multiple features per branch
        let exp: Experiment = serde_json::from_value(json!({
            "schemaVersion": "1.0.0",
            "slug": "secure-gold",
            "appName": "fenix",
            "appId": "bobo",
            "channel": "nightly",
            "endDate": null,
            "branches":[
                {
                    "slug": "control",
                    "ratio": 1,
                    "feature": {
                        "featureId": "feature1",
                        "enabled": true,
                        "value": {
                            "key": "value"
                        }
                    }
                },
                {
                    "slug": "treatment",
                    "ratio":1,
                    "feature": {
                        "featureId": "feature2",
                        "enabled": true,
                        "value": {
                            "key": "value2"
                        }
                    }
                }
            ],
            "probeSets":[],
            "startDate":null,
            "application":"fenix",
            "bucketConfig":{
                "count":10_000,
                "start":0,
                "total":10_000,
                "namespace":"secure-gold",
                "randomizationUnit":"nimbus_id"
            },
            "userFacingName":"Diagnostic test experiment",
            "referenceBranch":"control",
            "isEnrollmentPaused":false,
            "proposedEnrollment":7,
            "userFacingDescription":"This is a test experiment for diagnostic purposes.",
            "id":"secure-gold",
            "last_modified":1_602_197_324_372i64
        }))
        .unwrap();
        assert_eq!(
            exp.branches[0].get_feature_configs(),
            vec![FeatureConfig {
                feature_id: "feature1".to_string(),
                value: vec![("key".to_string(), json!("value"))]
                    .into_iter()
                    .collect()
            }]
        );
        assert_eq!(
            exp.branches[1].get_feature_configs(),
            vec![FeatureConfig {
                feature_id: "feature2".to_string(),
                value: vec![("key".to_string(), json!("value2"))]
                    .into_iter()
                    .collect()
            }]
        );
        assert!(exp.branches[0].features.is_none());
        assert!(exp.branches[1].features.is_none());
    }

    #[test]
    // This was the `Experiment` object schema as it originally shipped to Fenix Nightly.
    // It was missing some fields that have since been added.
    fn test_experiment_schema_initial_release() {
        // ⚠️ Warning : Do not change the JSON data used by this test. ⚠️
        let exp: Experiment = serde_json::from_value(json!({
            "schemaVersion": "1.0.0",
            "slug": "secure-gold",
            "endDate": null,
            "branches":[
                {
                    "slug": "control",
                    "ratio": 1,
                },
                {
                    "slug": "treatment",
                    "ratio":1,
                }
            ],
            "probeSets":[],
            "startDate":null,
            "application":"fenix",
            "bucketConfig":{
                "count":10_000,
                "start":0,
                "total":10_000,
                "namespace":"secure-gold",
                "randomizationUnit":"nimbus_id"
            },
            "userFacingName":"Diagnostic test experiment",
            "referenceBranch":"control",
            "isEnrollmentPaused":false,
            "proposedEnrollment":7,
            "userFacingDescription":"This is a test experiment for diagnostic purposes.",
            "id":"secure-gold",
            "last_modified":1_602_197_324_372i64
        }))
        .unwrap();
        assert!(exp.get_feature_ids().is_empty());
    }

    // In #96 we added a `featureIds` field to the Experiment schema.
    // This tests the data as it was after that change.
    #[test]
    fn test_experiment_schema_with_feature_ids() {
        // ⚠️ Warning : Do not change the JSON data used by this test. ⚠️
        let exp: Experiment = serde_json::from_value(json!({
            "schemaVersion": "1.0.0",
            "slug": "secure-gold",
            "endDate": null,
            "featureIds": ["some_control"],
            "branches":[
                {
                    "slug": "control",
                    "ratio": 1,
                    "feature": {
                        "featureId": "some_control",
                        "enabled": false
                    }
                },
                {
                    "slug": "treatment",
                    "ratio":1,
                    "feature": {
                        "featureId": "some_control",
                        "enabled": true
                    }
                }
            ],
            "probeSets":[],
            "startDate":null,
            "application":"fenix",
            "bucketConfig":{
                "count":10_000,
                "start":0,
                "total":10_000,
                "namespace":"secure-gold",
                "randomizationUnit":"nimbus_id"
            },
            "userFacingName":"Diagnostic test experiment",
            "referenceBranch":"control",
            "isEnrollmentPaused":false,
            "proposedEnrollment":7,
            "userFacingDescription":"This is a test experiment for diagnostic purposes.",
            "id":"secure-gold",
            "last_modified":1_602_197_324_372i64
        }))
        .unwrap();
        assert_eq!(exp.get_feature_ids(), vec!["some_control"]);
    }

    // In #97 we deprecated `application` and added `app_name`, `app_id`,
    // and `channel`.  This tests the ability to deserialize both variants.
    #[test]
    fn test_experiment_schema_with_adr0004_changes() {
        // ⚠️ Warning : Do not change the JSON data used by this test. ⚠️

        // First, test deserializing an `application` format experiment
        // to ensure the presence of `application` doesn't fail.
        let exp: Experiment = serde_json::from_value(json!({
            "schemaVersion": "1.0.0",
            "slug": "secure-gold",
            "endDate": null,
            "featureIds": ["some_control"],
            "branches":[
                {
                    "slug": "control",
                    "ratio": 1,
                    "feature": {
                        "featureId": "some_control",
                        "enabled": false
                    }
                },
                {
                    "slug": "treatment",
                    "ratio":1,
                    "feature": {
                        "featureId": "some_control",
                        "enabled": true
                    }
                }
            ],
            "probeSets":[],
            "startDate":null,
            "application":"fenix",
            "bucketConfig":{
                "count":10_000,
                "start":0,
                "total":10_000,
                "namespace":"secure-gold",
                "randomizationUnit":"nimbus_id"
            },
            "userFacingName":"Diagnostic test experiment",
            "referenceBranch":"control",
            "isEnrollmentPaused":false,
            "proposedEnrollment":7,
            "userFacingDescription":"This is a test experiment for diagnostic purposes.",
            "id":"secure-gold",
            "last_modified":1_602_197_324_372i64
        }))
        .unwrap();
        // Without the fields in the experiment, the resulting fields in the struct
        // should be `None`
        assert_eq!(exp.app_name, None);
        assert_eq!(exp.app_id, None);
        assert_eq!(exp.channel, None);

        // Next, test deserializing an experiment with `app_name`, `app_id`,
        // and `channel`.
        let exp: Experiment = serde_json::from_value(json!({
            "schemaVersion": "1.0.0",
            "slug": "secure-gold",
            "endDate": null,
            "featureIds": ["some_control"],
            "branches":[
                {
                    "slug": "control",
                    "ratio": 1,
                    "feature": {
                        "featureId": "some_control",
                        "enabled": false
                    }
                },
                {
                    "slug": "treatment",
                    "ratio":1,
                    "feature": {
                        "featureId": "some_control",
                        "enabled": true
                    }
                }
            ],
            "probeSets":[],
            "startDate":null,
            "appName":"fenix",
            "appId":"org.mozilla.fenix",
            "channel":"nightly",
            "bucketConfig":{
                "count":10_000,
                "start":0,
                "total":10_000,
                "namespace":"secure-gold",
                "randomizationUnit":"nimbus_id"
            },
            "userFacingName":"Diagnostic test experiment",
            "referenceBranch":"control",
            "isEnrollmentPaused":false,
            "proposedEnrollment":7,
            "userFacingDescription":"This is a test experiment for diagnostic purposes.",
            "id":"secure-gold",
            "last_modified":1_602_197_324_372i64
        }))
        .unwrap();
        assert_eq!(exp.app_name, Some("fenix".to_string()));
        assert_eq!(exp.app_id, Some("org.mozilla.fenix".to_string()));
        assert_eq!(exp.channel, Some("nightly".to_string()));

        // Finally, test deserializing an experiment with `app_name`, `app_id`,
        // `channel` AND `application` to ensure nothing fails.
        let exp: Experiment = serde_json::from_value(json!({
            "schemaVersion": "1.0.0",
            "slug": "secure-gold",
            "endDate": null,
            "featureIds": ["some_control"],
            "branches":[
                {
                    "slug": "control",
                    "ratio": 1,
                    "feature": {
                        "featureId": "some_control",
                        "enabled": false
                    }
                },
                {
                    "slug": "treatment",
                    "ratio":1,
                    "feature": {
                        "featureId": "some_control",
                        "enabled": true
                    }
                }
            ],
            "probeSets":[],
            "startDate":null,
            "application":"org.mozilla.fenix",
            "appName":"fenix",
            "appId":"org.mozilla.fenix",
            "channel":"nightly",
            "bucketConfig":{
                "count":10_000,
                "start":0,
                "total":10_000,
                "namespace":"secure-gold",
                "randomizationUnit":"nimbus_id"
            },
            "userFacingName":"Diagnostic test experiment",
            "referenceBranch":"control",
            "isEnrollmentPaused":false,
            "proposedEnrollment":7,
            "userFacingDescription":"This is a test experiment for diagnostic purposes.",
            "id":"secure-gold",
            "last_modified":1_602_197_324_372i64
        }))
        .unwrap();
        assert_eq!(exp.app_name, Some("fenix".to_string()));
        assert_eq!(exp.app_id, Some("org.mozilla.fenix".to_string()));
        assert_eq!(exp.channel, Some("nightly".to_string()));
    }
}

#[cfg(test)]
mod test_schema_deserialization {
    use super::*;

    use serde_json::{json, Map, Value};

    #[derive(Deserialize, Serialize, Debug, Clone)]
    #[serde(rename_all = "camelCase")]
    pub struct FeatureConfigProposed {
        pub enabled: bool,
        pub feature_id: String,
        #[serde(default)]
        pub value: Map<String, Value>,
    }

    #[test]
    fn test_deserialize_untyped_json() -> Result<()> {
        let without_value = serde_json::from_value::<FeatureConfig>(json!(
            {
                "featureId": "some_control",
                "enabled": true,
            }
        ))?;

        let with_object_value = serde_json::from_value::<FeatureConfig>(json!(
            {
                "featureId": "some_control",
                "enabled": true,
                "value": {
                    "color": "blue",
                },
            }
        ))?;

        assert_eq!(
            serde_json::to_string(&without_value.value)?,
            "{}".to_string()
        );
        assert_eq!(
            serde_json::to_string(&with_object_value.value)?,
            "{\"color\":\"blue\"}"
        );
        assert_eq!(with_object_value.value.get("color").unwrap(), "blue");

        let rejects_scalar_value = serde_json::from_value::<FeatureConfig>(json!(
            {
                "featureId": "some_control",
                "enabled": true,
                "value": 1,
            }
        ))
        .is_err();

        assert!(rejects_scalar_value);

        let rejects_array_value = serde_json::from_value::<FeatureConfig>(json!(
            {
                "featureId": "some_control",
                "enabled": true,
                "value": [1, 2, 3],
            }
        ))
        .is_err();

        assert!(rejects_array_value);

        Ok(())
    }
}
