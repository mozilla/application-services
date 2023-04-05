// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod behavior;
mod dbcache;
mod enrollment;
pub mod error;
mod evaluator;
use behavior::EventStore;
use chrono::{DateTime, NaiveDateTime, Utc};
use defaults::Defaults;
pub use error::{NimbusError, Result};
mod client;
mod config;
mod defaults;
mod matcher;
pub mod persistence;
mod sampling;
mod strings;
mod updating;
pub mod versioning;
#[cfg(debug_assertions)]
pub use evaluator::evaluate_enrollment;

use client::{create_client, parse_experiments, SettingsClient};
pub use config::RemoteSettingsConfig;
use dbcache::DatabaseCache;
pub use enrollment::EnrollmentStatus;
use enrollment::{
    get_global_user_participation, opt_in_with_branch, opt_out, set_global_user_participation,
    EnrolledFeature, EnrollmentChangeEvent, EnrollmentsEvolver,
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
use serde::Serialize;
use serde_derive::*;
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use updating::{read_and_remove_pending_experiments, write_pending_experiments};
use uuid::Uuid;

#[cfg(test)]
mod tests;

const DEFAULT_TOTAL_BUCKETS: u32 = 10000;
const DB_KEY_NIMBUS_ID: &str = "nimbus-id";
pub const DB_KEY_INSTALLATION_DATE: &str = "installation-date";
pub const DB_KEY_UPDATE_DATE: &str = "update-date";
pub const DB_KEY_APP_VERSION: &str = "app-version";

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
    event_store: Arc<Mutex<EventStore>>,
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
            event_store: Arc::default(),
        })
    }

    pub fn with_targeting_attributes(&mut self, targeting_attributes: TargetingAttributes) {
        let mut state = self.mutable_state.lock().unwrap();
        state.targeting_attributes = targeting_attributes;
    }

    pub fn get_targeting_attributes(&self) -> TargetingAttributes {
        let state = self.mutable_state.lock().unwrap();
        state.targeting_attributes.clone()
    }

    pub fn initialize(&self) -> Result<()> {
        let db = self.db()?;
        // We're not actually going to write, we just want to exclude concurrent writers.
        let mut writer = db.write()?;

        let mut state = self.mutable_state.lock().unwrap();
        self.begin_initialize(db, &mut writer, &mut state)?;
        self.end_initialize(db, writer, &mut state)?;

        Ok(())
    }

    // These are tasks which should be in the initialize and apply_pending_experiments
    // but should happen before the enrollment calculations are done.
    fn begin_initialize(
        &self,
        db: &Database,
        writer: &mut Writer,
        state: &mut MutexGuard<InternalMutableState>,
    ) -> Result<()> {
        self.update_ta_install_dates(db, writer, state)?;
        self.event_store.lock().unwrap().read_from_db(db)?;
        Ok(())
    }

    // These are tasks which should be in the initialize and apply_pending_experiments
    // but should happen after the enrollment calculations are done.
    fn end_initialize(
        &self,
        db: &Database,
        writer: Writer,
        state: &mut MutexGuard<InternalMutableState>,
    ) -> Result<()> {
        self.update_ta_active_experiments(db, &writer, state)?;
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
        let mut state = self.mutable_state.lock().unwrap();
        set_global_user_participation(db, &mut writer, user_participating)?;

        let existing_experiments: Vec<Experiment> =
            db.get_store(StoreId::Experiments).collect_all(&writer)?;
        // We pass the existing experiments as "updated experiments"
        // to the evolver.
        let events = self.evolve_experiments(db, &mut writer, &mut state, &existing_experiments)?;
        self.end_initialize(db, writer, &mut state)?;
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
        let th = self.create_targeting_helper(None)?;
        Ok(self
            .get_all_experiments()?
            .into_iter()
            .filter(|exp| is_experiment_available(&th, exp, false))
            .map(|exp| exp.into())
            .collect())
    }

    pub fn get_enrollment_by_feature(&self, feature_id: String) -> Result<Option<EnrolledFeature>> {
        self.database_cache.get_enrollment_by_feature(&feature_id)
    }

    pub fn opt_in_with_branch(
        &self,
        experiment_slug: String,
        branch: String,
    ) -> Result<Vec<EnrollmentChangeEvent>> {
        let db = self.db()?;
        let mut writer = db.write()?;
        let result = opt_in_with_branch(db, &mut writer, &experiment_slug, &branch)?;
        let mut state = self.mutable_state.lock().unwrap();
        self.end_initialize(db, writer, &mut state)?;
        Ok(result)
    }

    pub fn opt_out(&self, experiment_slug: String) -> Result<Vec<EnrollmentChangeEvent>> {
        let db = self.db()?;
        let mut writer = db.write()?;
        let result = opt_out(db, &mut writer, &experiment_slug)?;
        let mut state = self.mutable_state.lock().unwrap();
        self.end_initialize(db, writer, &mut state)?;
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

    /**
     * Calculate the days since install and days since update on the targeting_attributes.
     */
    fn update_ta_install_dates(
        &self,
        db: &Database,
        writer: &mut Writer,
        state: &mut MutexGuard<InternalMutableState>,
    ) -> Result<()> {
        let installation_date = self.get_installation_date(db, writer)?;
        log::info!("[Nimbus] Installation Date: {}", installation_date);
        let update_date = self.get_update_date(db, writer)?;
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
        if state.targeting_attributes.days_since_install.is_none() {
            state.targeting_attributes.days_since_install =
                Some(duration_since_install.num_days() as i32);
        }
        if state.targeting_attributes.days_since_update.is_none() {
            state.targeting_attributes.days_since_update =
                Some(duration_since_update.num_days() as i32);
        }

        Ok(())
    }

    /**
     * Calculates the active_experiments based on current enrollments for the targeting attributes.
     */
    fn update_ta_active_experiments(
        &self,
        db: &Database,
        writer: &Writer,
        state: &mut MutexGuard<InternalMutableState>,
    ) -> Result<()> {
        let enrollments_store = db.get_store(StoreId::Enrollments);
        let prev_enrollments: Vec<enrollment::ExperimentEnrollment> =
            enrollments_store.collect_all(writer)?;

        let mut set = HashSet::<String>::new();
        for ee in prev_enrollments {
            if let EnrollmentStatus::Enrolled { .. } = ee.status {
                set.insert(ee.slug.clone());
            }
        }

        state.targeting_attributes.active_experiments = set;

        Ok(())
    }

    fn evolve_experiments(
        &self,
        db: &Database,
        writer: &mut Writer,
        state: &mut InternalMutableState,
        experiments: &[Experiment],
    ) -> Result<Vec<EnrollmentChangeEvent>> {
        let nimbus_id = self.read_or_create_nimbus_id(db, writer)?;
        let targeting_helper =
            NimbusTargetingHelper::new(&state.targeting_attributes, self.event_store.clone());
        let evolver = EnrollmentsEvolver::new(
            &nimbus_id,
            &state.available_randomization_units,
            &targeting_helper,
        );
        evolver.evolve_enrollments_in_db(db, writer, experiments)
    }

    pub fn apply_pending_experiments(&self) -> Result<Vec<EnrollmentChangeEvent>> {
        log::info!("updating experiment list");
        let db = self.db()?;
        let mut writer = db.write()?;

        // We'll get the pending experiments which were stored for us, either by fetch_experiments
        // or by set_experiments_locally.
        let pending_updates = read_and_remove_pending_experiments(db, &mut writer)?;
        let mut state = self.mutable_state.lock().unwrap();
        self.begin_initialize(db, &mut writer, &mut state)?;

        let res = match pending_updates {
            Some(new_experiments) => {
                self.update_ta_active_experiments(db, &writer, &mut state)?;
                // Perform the enrollment calculations if there are pending experiments.
                self.evolve_experiments(db, &mut writer, &mut state, &new_experiments)?
            }
            None => vec![],
        };

        // Finish up any cleanup, e.g. copying from database in to memory.
        self.end_initialize(db, writer, &mut state)?;
        Ok(res)
    }

    fn get_installation_date(&self, db: &Database, writer: &mut Writer) -> Result<DateTime<Utc>> {
        // we first check our context
        if let Some(context_installation_date) = self.app_context.installation_date {
            let res = DateTime::<Utc>::from_utc(
                NaiveDateTime::from_timestamp_opt(context_installation_date / 1_000, 0).unwrap(),
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
        let mut state = self.mutable_state.lock().unwrap();
        // If we have no `nimbus_id` when we can safely assume that there's
        // no other experiment state that needs to be reset.
        let store = db.get_store(StoreId::Meta);
        if store.get::<String, _>(&writer, DB_KEY_NIMBUS_ID)?.is_some() {
            // Each enrollment state includes a unique `enrollment_id` which we need to clear.
            events = enrollment::reset_telemetry_identifiers(db, &mut writer)?;

            // Remove any stored event counts
            db.clear_event_count_data(&mut writer)?;

            // The `nimbus_id` itself is a unique identifier.
            // N.B. we do this last, as a signal that all data has been reset.
            store.delete(&mut writer, DB_KEY_NIMBUS_ID)?;
            self.end_initialize(db, writer, &mut state)?;
        }

        // (No need to commit `writer` if the above check was false, since we didn't change anything)
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

    fn merge_additional_context(&self, context: Option<JsonObject>) -> Result<Value> {
        let context = context.map(Value::Object);
        let targeting = serde_json::to_value(self.get_targeting_attributes())?;
        let context = match context {
            Some(v) => v.defaults(&targeting)?,
            None => targeting,
        };

        Ok(context)
    }

    pub fn create_targeting_helper(
        &self,
        additional_context: Option<JsonObject>,
    ) -> Result<Arc<NimbusTargetingHelper>> {
        let context = self.merge_additional_context(additional_context)?;
        let helper = NimbusTargetingHelper::new(context, self.event_store.clone());
        Ok(Arc::new(helper))
    }

    pub fn create_string_helper(
        &self,
        additional_context: Option<JsonObject>,
    ) -> Result<Arc<NimbusStringHelper>> {
        let context = self.merge_additional_context(additional_context)?;
        let helper = NimbusStringHelper::new(context.as_object().unwrap().to_owned());
        Ok(Arc::new(helper))
    }

    /// Records an event for the purposes of behavioral targeting.
    ///
    /// This function is used to record and persist data used for the behavioral
    /// targeting such as "core-active" user targeting.
    pub fn record_event(&self, event_id: String, count: i64) -> Result<()> {
        let mut event_store = self.event_store.lock().unwrap();
        event_store.record_event(count as u64, &event_id, None)?;
        event_store.persist_data(self.db()?)?;
        Ok(())
    }

    /// Records an event for the purposes of behavioral targeting.
    ///
    /// This differs from the `record_event` method in that the event is recorded as if it were
    /// recorded `seconds_ago` in the past. This makes it very useful for testing.
    pub fn record_past_event(&self, event_id: String, seconds_ago: i64, count: i64) -> Result<()> {
        let mut event_store = self.event_store.lock().unwrap();
        event_store.record_past_event(
            count as u64,
            &event_id,
            None,
            chrono::Duration::seconds(seconds_ago),
        )?;
        event_store.persist_data(self.db()?)?;
        Ok(())
    }

    /// Clear all events in the Nimbus event store.
    ///
    /// This should only be used in testing or cases where the previous event store is no longer viable.
    pub fn clear_events(&self) -> Result<()> {
        let mut event_store = self.event_store.lock().unwrap();
        event_store.clear(self.db()?)?;
        Ok(())
    }

    pub fn event_store(&self) -> Arc<Mutex<EventStore>> {
        self.event_store.clone()
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

    pub(crate) fn is_rollout(&self) -> bool {
        self.is_rollout
    }
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

pub struct NimbusStringHelper {
    context: JsonObject,
}

impl NimbusStringHelper {
    fn new(context: JsonObject) -> Self {
        Self { context }
    }

    pub fn get_uuid(&self, template: String) -> Option<String> {
        if template.contains("{uuid}") {
            let uuid = Uuid::new_v4();
            Some(uuid.to_string())
        } else {
            None
        }
    }

    pub fn string_format(&self, template: String, uuid: Option<String>) -> String {
        match uuid {
            Some(uuid) => {
                let mut map = self.context.clone();
                map.insert("uuid".to_string(), Value::String(uuid));
                strings::fmt_with_map(&template, &map)
            }
            _ => strings::fmt_with_map(&template, &self.context),
        }
    }
}

pub struct NimbusTargetingHelper {
    context: Value,
    event_store: Arc<Mutex<EventStore>>,
}

impl NimbusTargetingHelper {
    pub fn new<C: Serialize>(context: C, event_store: Arc<Mutex<EventStore>>) -> Self {
        Self {
            context: serde_json::to_value(context).unwrap(),
            event_store,
        }
    }

    pub fn eval_jexl(&self, expr: String) -> Result<bool> {
        evaluator::jexl_eval(&expr, &self.context, self.event_store.clone())
    }

    pub(crate) fn put(&self, key: &str, value: bool) -> Self {
        let context = if let Value::Object(map) = &self.context {
            let mut map = map.clone();
            map.insert(key.to_string(), Value::Bool(value));
            Value::Object(map)
        } else {
            self.context.clone()
        };

        let event_store = self.event_store.clone();
        Self {
            context,
            event_store,
        }
    }
}

type JsonObject = Map<String, Value>;

#[cfg(feature = "uniffi-bindings")]
impl UniffiCustomTypeConverter for JsonObject {
    type Builtin = String;

    fn into_custom(val: Self::Builtin) -> uniffi::Result<Self> {
        let json: Value = serde_json::from_str(&val)?;

        match json.as_object() {
            Some(obj) => Ok(obj.clone()),
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
include!(concat!(env!("OUT_DIR"), "/nimbus.uniffi.rs"));
