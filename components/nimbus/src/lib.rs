// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod dbcache;
mod enrollment;
pub mod error;
mod evaluator;
pub use error::{NimbusError, Result};
mod client;
mod config;
mod matcher;
mod persistence;
mod sampling;
mod updating;
#[cfg(debug_assertions)]
pub use evaluator::evaluate_enrollment;

use client::{create_client, parse_experiments, SettingsClient};
pub use config::RemoteSettingsConfig;
use dbcache::DatabaseCache;
pub use enrollment::EnrollmentStatus;
use enrollment::{
    get_enrollments, get_global_user_participation, opt_in_with_branch, opt_out,
    set_global_user_participation, EnrollmentChangeEvent, EnrollmentChangeEventType,
    EnrollmentsEvolver,
};
pub use matcher::AppContext;
use once_cell::sync::OnceCell;
use persistence::{Database, StoreId, Writer};
use serde_derive::*;
use std::path::PathBuf;
use std::sync::Mutex;
use updating::{read_and_remove_pending_experiments, write_pending_experiments};
use uuid::Uuid;

const DEFAULT_TOTAL_BUCKETS: u32 = 10000;
const DB_KEY_NIMBUS_ID: &str = "nimbus-id";

// The main `NimbusClient` struct must not expose any methods that make an `&mut self`,
// in order to be compatible with the uniffi `[Threadsafe]` annotation. This is a helper
// struct to contain the bits that do actually need to be mutable, so they can be
// protected by a Mutex.
#[derive(Default)]
struct InternalMutableState {
    available_randomization_units: AvailableRandomizationUnits,
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

    pub fn initialize(&self) -> Result<()> {
        let db = self.db()?;
        // We're not actually going to write, we just want to exclude concurrent writers.
        let writer = db.write()?;
        self.database_cache.commit_and_update(&db, writer)?;
        Ok(())
    }

    // Note: the contract for this function is that it never blocks on IO.
    pub fn get_experiment_branch(&self, slug: String) -> Result<Option<String>> {
        self.database_cache.get_experiment_branch(&slug)
    }

    pub fn get_experiment_branches(&self, slug: String) -> Result<Vec<Branch>> {
        Ok(self
            .get_all_experiments()?
            .iter()
            .find(|e| e.slug == slug)
            .map(|e| e.branches.clone())
            .ok_or(NimbusError::NoSuchExperiment(slug))?)
    }

    pub fn get_global_user_participation(&self) -> Result<bool> {
        let db = self.db()?;
        let reader = db.read()?;
        get_global_user_participation(&db, &reader)
    }

    pub fn set_global_user_participation(
        &self,
        user_participating: bool,
    ) -> Result<Vec<EnrollmentChangeEvent>> {
        let db = self.db()?;
        let mut writer = db.write()?;
        set_global_user_participation(&db, &mut writer, user_participating)?;

        let existing_experiments: Vec<Experiment> =
            db.get_store(StoreId::Experiments).collect_all(&writer)?;
        // We pass the existing experiments as "updated experiments"
        // to the evolver.
        let nimbus_id = self.read_or_create_nimbus_id(&db, &mut writer)?;
        let state = self.mutable_state.lock().unwrap();
        let evolver = EnrollmentsEvolver::new(
            &nimbus_id,
            &state.available_randomization_units,
            &self.app_context,
        );
        let events = evolver.evolve_enrollments_in_db(&db, &mut writer, &existing_experiments)?;
        self.database_cache.commit_and_update(&db, writer)?;
        Ok(events)
    }

    pub fn get_active_experiments(&self) -> Result<Vec<EnrolledExperiment>> {
        let db = self.db()?;
        let reader = db.read()?;
        get_enrollments(&db, &reader)
    }

    pub fn get_all_experiments(&self) -> Result<Vec<Experiment>> {
        let db = self.db()?;
        let reader = db.read()?;
        db.get_store(StoreId::Experiments)
            .collect_all::<Experiment, _>(&reader)
    }

    pub fn opt_in_with_branch(
        &self,
        experiment_slug: String,
        branch: String,
    ) -> Result<Vec<EnrollmentChangeEvent>> {
        let db = self.db()?;
        let mut writer = db.write()?;
        let result = opt_in_with_branch(&db, &mut writer, &experiment_slug, &branch)?;
        self.database_cache.commit_and_update(&db, writer)?;
        Ok(result)
    }

    pub fn opt_out(&self, experiment_slug: String) -> Result<Vec<EnrollmentChangeEvent>> {
        let db = self.db()?;
        let mut writer = db.write()?;
        let result = opt_out(&db, &mut writer, &experiment_slug)?;
        self.database_cache.commit_and_update(&db, writer)?;
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
        write_pending_experiments(&db, &mut writer, new_experiments)?;
        writer.commit()?;
        Ok(())
    }

    pub fn apply_pending_experiments(&self) -> Result<Vec<EnrollmentChangeEvent>> {
        log::info!("updating experiment list");
        let db = self.db()?;
        let mut writer = db.write()?;
        let pending_updates = read_and_remove_pending_experiments(&db, &mut writer)?;
        Ok(match pending_updates {
            Some(new_experiments) => {
                let nimbus_id = self.read_or_create_nimbus_id(&db, &mut writer)?;
                let state = self.mutable_state.lock().unwrap();
                let evolver = EnrollmentsEvolver::new(
                    &nimbus_id,
                    &state.available_randomization_units,
                    &self.app_context,
                );
                let events =
                    evolver.evolve_enrollments_in_db(&db, &mut writer, &new_experiments)?;
                self.database_cache.commit_and_update(&db, writer)?;
                events
            }
            // We don't need to writer.commit() here because we haven't done anything.
            None => vec![],
        })
    }

    pub fn set_experiments_locally(&self, experiments_json: String) -> Result<()> {
        let new_experiments = parse_experiments(&experiments_json)?;
        let db = self.db()?;
        let mut writer = db.write()?;
        write_pending_experiments(&db, &mut writer, new_experiments)?;
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
            self.database_cache.commit_and_update(&db, writer)?;
        }
        // (No need to commit `writer` if the above check was false, since we didn't change anything)
        let mut state = self.mutable_state.lock().unwrap();
        state.available_randomization_units = new_randomization_units;
        Ok(events)
    }

    pub fn nimbus_id(&self) -> Result<Uuid> {
        let db = self.db()?;
        let mut writer = db.write()?;
        let uuid = self.read_or_create_nimbus_id(&db, &mut writer)?;
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
        self.db
            .get_or_try_init(|| Ok(Database::new(&self.db_path)?))
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
    pub application: String,
    pub user_facing_name: String,
    pub user_facing_description: String,
    pub is_enrollment_paused: bool,
    pub bucket_config: BucketConfig,
    pub probe_sets: Vec<String>,
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
    // N.B. records in RemoteSettings will have `id` and `filter_expression` fields,
    // but we ignore them because they're for internal use by RemoteSettings.
}

impl Experiment {
    fn has_branch(&self, branch_slug: &str) -> bool {
        self.branches
            .iter()
            .any(|branch| branch.slug == branch_slug)
    }

    fn get_first_feature_id(&self) -> String {
        if self.feature_ids.is_empty() {
            "".to_string()
        } else {
            self.feature_ids[0].clone()
        }
    }
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

// ⚠️ Attention : Changes to this type should be accompanied by a new test  ⚠️
// ⚠️ in `mod test_schema_bw_compat` below, and may require a DB migration. ⚠️
#[derive(Deserialize, Serialize, Debug, Default, Clone, PartialEq)]
pub struct Branch {
    pub slug: String,
    pub ratio: i32,
    pub feature: Option<FeatureConfig>,
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
    use super::*;
    use enrollment::{EnrolledReason, EnrollmentStatus, ExperimentEnrollment};
    use tempdir::TempDir;

    #[test]
    fn test_telemetry_reset() -> Result<()> {
        let mock_client_id = "client-1".to_string();
        let mock_exp_slug = "exp-1".to_string();
        let mock_exp_branch = "branch-1".to_string();
        let mock_feature_id = "feature-1".to_string();

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
                status: EnrollmentStatus::new_enrolled(
                    EnrolledReason::Qualified,
                    &mock_exp_branch,
                    &mock_feature_id,
                ),
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
        assert_eq!(exp.get_first_feature_id(), "");
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
        assert_eq!(exp.get_first_feature_id(), "some_control");
    }
}
