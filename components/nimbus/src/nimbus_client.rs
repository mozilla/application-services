// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::{
    behavior::EventStore,
    client::{create_client, SettingsClient},
    dbcache::DatabaseCache,
    defaults::Defaults,
    enrollment::{
        get_global_user_participation, opt_in_with_branch, opt_out, reset_telemetry_identifiers,
        set_global_user_participation, EnrolledFeature, EnrollmentChangeEvent,
        EnrollmentChangeEventType, EnrollmentStatus, EnrollmentsEvolver, ExperimentEnrollment,
    },
    error::BehaviorError,
    evaluator::{is_experiment_available, TargetingAttributes},
    matcher::AppContext,
    persistence::{Database, StoreId, Writer},
    schema::parse_experiments,
    strings::fmt_with_map,
    updating::{read_and_remove_pending_experiments, write_pending_experiments},
    AvailableExperiment, AvailableRandomizationUnits, EnrolledExperiment, Experiment,
    ExperimentBranch, NimbusError, NimbusTargetingHelper, Result,
};
use chrono::{DateTime, NaiveDateTime, Utc};
use once_cell::sync::OnceCell;
use remote_settings::RemoteSettingsConfig;
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use uuid::Uuid;

const DB_KEY_NIMBUS_ID: &str = "nimbus-id";
pub const DB_KEY_INSTALLATION_DATE: &str = "installation-date";
pub const DB_KEY_UPDATE_DATE: &str = "update-date";
pub const DB_KEY_APP_VERSION: &str = "app-version";
pub const DB_KEY_FETCH_ENABLED: &str = "fetch-enabled";

// The main `NimbusClient` struct must not expose any methods that make an `&mut self`,
// in order to be compatible with the uniffi's requirements on objects. This is a helper
// struct to contain the bits that do actually need to be mutable, so they can be
// protected by a Mutex.
#[derive(Default)]
pub struct InternalMutableState {
    pub(crate) available_randomization_units: AvailableRandomizationUnits,
    // Application level targeting attributes
    targeting_attributes: TargetingAttributes,
}

/// Nimbus is the main struct representing the experiments state
/// It should hold all the information needed to communicate a specific user's
/// experimentation status
pub struct NimbusClient {
    settings_client: Mutex<Box<dyn SettingsClient + Send>>,
    pub(crate) mutable_state: Mutex<InternalMutableState>,
    app_context: AppContext,
    pub(crate) db: OnceCell<Database>,
    // Manages an in-memory cache so that we can answer certain requests
    // without doing (or waiting for) IO.
    database_cache: DatabaseCache,
    db_path: PathBuf,
    coenrolling_feature_ids: Vec<String>,
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
            // With this being default, i.e. empty, Nimbus doesn't support coenrolling ids.
            // Once the API is connected with Swift/Kotlin it will.
            // This is the subject of https://mozilla-hub.atlassian.net/browse/EXP-3623
            coenrolling_feature_ids: Default::default(),
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
        let coenrolling_ids = self
            .coenrolling_feature_ids
            .iter()
            .map(|s| s.as_str())
            .collect();
        self.database_cache
            .commit_and_update(db, writer, &coenrolling_ids)?;
        Ok(())
    }

    pub fn get_enrollment_by_feature(&self, feature_id: String) -> Result<Option<EnrolledFeature>> {
        self.database_cache.get_enrollment_by_feature(&feature_id)
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

    pub fn fetch_experiments(&self) -> Result<()> {
        if !self.is_fetch_enabled()? {
            return Ok(());
        }
        log::info!("fetching experiments");
        let settings_client = self.settings_client.lock().unwrap();
        let new_experiments = settings_client.fetch_experiments()?;
        let db = self.db()?;
        let mut writer = db.write()?;
        write_pending_experiments(db, &mut writer, new_experiments)?;
        writer.commit()?;
        Ok(())
    }

    pub fn set_fetch_enabled(&self, allow: bool) -> Result<()> {
        let db = self.db()?;
        let mut writer = db.write()?;
        db.get_store(StoreId::Meta)
            .put(&mut writer, DB_KEY_FETCH_ENABLED, &allow)?;
        writer.commit()?;
        Ok(())
    }

    pub(crate) fn is_fetch_enabled(&self) -> Result<bool> {
        let db = self.db()?;
        let reader = db.read()?;
        let enabled = db
            .get_store(StoreId::Meta)
            .get(&reader, DB_KEY_FETCH_ENABLED)?
            .unwrap_or(true);
        Ok(enabled)
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
        let prev_enrollments: Vec<ExperimentEnrollment> = enrollments_store.collect_all(writer)?;

        let mut is_enrolled_set = HashSet::<String>::new();
        let mut all_enrolled_set = HashSet::<String>::new();
        for ee in prev_enrollments {
            match ee.status {
                EnrollmentStatus::Enrolled { .. } => {
                    is_enrolled_set.insert(ee.slug.clone());
                    all_enrolled_set.insert(ee.slug.clone());
                }
                EnrollmentStatus::WasEnrolled { .. } => {
                    all_enrolled_set.insert(ee.slug.clone());
                }
                _ => {}
            }
        }

        state.targeting_attributes.active_experiments = is_enrolled_set;
        state.targeting_attributes.enrollments = all_enrolled_set;

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
        let coenrolling_feature_ids = self
            .coenrolling_feature_ids
            .iter()
            .map(|s| s.as_str())
            .collect();
        let evolver = EnrollmentsEvolver::new(
            &nimbus_id,
            &state.available_randomization_units,
            &targeting_helper,
            &coenrolling_feature_ids,
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

    /// Reset all enrollments and experiments in the database.
    ///
    /// This should only be used in testing.
    pub fn reset_enrollments(&self) -> Result<()> {
        let db = self.db()?;
        let mut writer = db.write()?;
        let mut state = self.mutable_state.lock().unwrap();
        db.clear_experiments_and_enrollments(&mut writer)?;
        self.end_initialize(db, writer, &mut state)?;
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
            events = reset_telemetry_identifiers(db, &mut writer)?;

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

    pub(crate) fn db(&self) -> Result<&Database> {
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
        if seconds_ago < 0 {
            return Err(NimbusError::BehaviorError(BehaviorError::InvalidDuration(
                "Time duration in the past must be positive".to_string(),
            )));
        }
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

    /// Advances the event store's concept of `now` artificially.
    ///
    /// This works alongside `record_event` and `record_past_event` for testing purposes.
    pub fn advance_event_time(&self, by_seconds: i64) -> Result<()> {
        if by_seconds < 0 {
            return Err(NimbusError::BehaviorError(BehaviorError::InvalidDuration(
                "Time duration in the future must be positive".to_string(),
            )));
        }
        let mut event_store = self.event_store.lock().unwrap();
        event_store.advance_datum(chrono::Duration::seconds(by_seconds));
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

    pub fn dump_state_to_log(&self) -> Result<()> {
        let experiments = self.get_active_experiments()?;
        log::info!("{0: <65}| {1: <30}| {2}", "Slug", "Features", "Branch");
        for exp in &experiments {
            log::info!(
                "{0: <65}| {1: <30}| {2}",
                &exp.slug,
                &exp.feature_ids.join(", "),
                &exp.branch_slug
            );
        }
        Ok(())
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
                fmt_with_map(&template, &map)
            }
            _ => fmt_with_map(&template, &self.context),
        }
    }
}

type JsonObject = Map<String, Value>;

#[cfg(feature = "stateful-uniffi-bindings")]
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

#[cfg(feature = "stateful-uniffi-bindings")]
uniffi::include_scaffolding!("nimbus");
