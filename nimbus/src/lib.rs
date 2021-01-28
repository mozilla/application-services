// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod dbcache;
mod enrollment;
pub mod error;
mod evaluator;
pub use error::{Error, Result};
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

/// Nimbus is the main struct representing the experiments state
/// It should hold all the information needed to communicate a specific user's
/// experimentation status
pub struct NimbusClient {
    settings_client: Mutex<Box<dyn SettingsClient + Send>>,
    available_randomization_units: AvailableRandomizationUnits,
    app_context: AppContext,
    db: OnceCell<Mutex<Database>>,
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
        Ok(Self {
            settings_client,
            available_randomization_units,
            app_context,
            database_cache: Default::default(),
            db_path: db_path.into(),
            db: OnceCell::default(),
        })
    }

    pub fn initialize(&self) -> Result<()> {
        let db = self.db()?.lock().unwrap();
        self.database_cache.update(&db)?;
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
            .ok_or(Error::NoSuchExperiment(slug))?)
    }

    pub fn get_global_user_participation(&self) -> Result<bool> {
        // This is a bit smelly, but get_global_user_participation() needs a
        // writer so that the implementation of update_enrollments can pass one
        // and see the correct value.
        let db = self.db()?.lock().unwrap();
        let writer = db.write()?;
        get_global_user_participation(&db, &writer)
    }

    pub fn set_global_user_participation(
        &self,
        user_participating: bool,
    ) -> Result<Vec<EnrollmentChangeEvent>> {
        let db = self.db()?.lock().unwrap();
        let mut writer = db.write()?;
        set_global_user_participation(&db, &mut writer, user_participating)?;

        let existing_experiments = db
            .get_store(StoreId::Experiments)
            .collect_all::<Experiment>(&writer)?;
        // We pass the existing experiments as "updated experiments"
        // to the evolver.
        let nimbus_id = self.read_or_create_nimbus_id(&db, &mut writer)?;
        let evolver = EnrollmentsEvolver::new(
            &nimbus_id,
            &self.available_randomization_units,
            &self.app_context,
        );
        let events = evolver.evolve_enrollments_in_db(&db, &mut writer, &existing_experiments)?;
        writer.commit()?;
        self.database_cache.update(&db)?;
        Ok(events)
    }

    pub fn get_active_experiments(&self) -> Result<Vec<EnrolledExperiment>> {
        let db = self.db()?.lock().unwrap();
        get_enrollments(&db)
    }

    pub fn get_all_experiments(&self) -> Result<Vec<Experiment>> {
        let db = self.db()?.lock().unwrap();
        db.collect_all(StoreId::Experiments)
    }

    pub fn opt_in_with_branch(
        &self,
        experiment_slug: String,
        branch: String,
    ) -> Result<Vec<EnrollmentChangeEvent>> {
        let db = self.db()?.lock().unwrap();
        let result = opt_in_with_branch(&db, &experiment_slug, &branch)?;
        self.database_cache.update(&db)?;
        Ok(result)
    }

    pub fn opt_out(&self, experiment_slug: String) -> Result<Vec<EnrollmentChangeEvent>> {
        let db = self.db()?.lock().unwrap();
        let result = opt_out(&db, &experiment_slug)?;
        self.database_cache.update(&db)?;
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
        let db = self.db()?.lock().unwrap();
        write_pending_experiments(&db, new_experiments)?;
        Ok(())
    }

    pub fn apply_pending_experiments(&self) -> Result<Vec<EnrollmentChangeEvent>> {
        log::info!("updating experiment list");
        let db = self.db()?.lock().unwrap();
        let mut writer = db.write()?;
        let pending_updates = read_and_remove_pending_experiments(&db, &mut writer)?;
        Ok(match pending_updates {
            Some(new_experiments) => {
                let nimbus_id = self.read_or_create_nimbus_id(&db, &mut writer)?;
                let evolver = EnrollmentsEvolver::new(
                    &nimbus_id,
                    &self.available_randomization_units,
                    &self.app_context,
                );
                let events =
                    evolver.evolve_enrollments_in_db(&db, &mut writer, &new_experiments)?;
                writer.commit()?;
                self.database_cache.update(&db)?;
                events
            }
            // We don't need to writer.commit() here because we haven't done anything.
            None => vec![],
        })
    }

    pub fn set_experiments_locally(&self, experiments_json: String) -> Result<()> {
        let new_experiments = parse_experiments(&experiments_json)?;
        let db = self.db()?.lock().unwrap();
        write_pending_experiments(&db, new_experiments)?;
        Ok(())
    }

    pub fn nimbus_id(&self) -> Result<Uuid> {
        let db = self.db()?.lock().unwrap();
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
        let db = self.db()?.lock().unwrap();
        let mut writer = db.write()?;
        db.get_store(StoreId::Meta)
            .put(&mut writer, DB_KEY_NIMBUS_ID, uuid)?;
        writer.commit()?;
        Ok(())
    }

    fn db(&self) -> Result<&Mutex<Database>> {
        self.db
            .get_or_try_init(|| Ok(Mutex::new(Database::new(&self.db_path)?)))
    }
}

#[derive(Debug, Clone)]
pub struct EnrolledExperiment {
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

// ⚠️ Warning : Altering this type might require a DB migration. ⚠️
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

// ⚠️ Warning : Altering this type might require a DB migration. ⚠️
#[derive(Deserialize, Serialize, Debug, Default, Clone, PartialEq)]
pub struct Branch {
    pub slug: String,
    pub ratio: i32,
    pub feature: Option<FeatureConfig>,
}

fn default_buckets() -> u32 {
    DEFAULT_TOTAL_BUCKETS
}

// ⚠️ Warning : Altering this type might require a DB migration. ⚠️
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

// ⚠️ Warning : Altering this type might require a DB migration. ⚠️
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
    dummy: i8, // See comments in nimbus.idl for why this hacky item exists.
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
