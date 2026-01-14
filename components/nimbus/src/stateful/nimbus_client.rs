/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::collections::HashSet;
use std::fmt::Debug;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use chrono::{DateTime, NaiveDateTime, Utc};
use once_cell::sync::OnceCell;
use remote_settings::RemoteSettingsService;
use serde_json::Value;
use uuid::Uuid;

use crate::defaults::Defaults;
use crate::enrollment::{
    EnrolledFeature, EnrollmentChangeEvent, EnrollmentChangeEventType, EnrollmentsEvolver,
    ExperimentEnrollment, PreviousGeckoPrefState,
};
use crate::error::{BehaviorError, info};
use crate::evaluator::{
    CalculatedAttributes, ExperimentAvailable, TargetingAttributes, get_calculated_attributes,
    is_experiment_available,
};
use crate::json::{JsonObject, PrefValue};
use crate::metrics::{
    EnrollmentStatusExtraDef, FeatureExposureExtraDef, MalformedFeatureConfigExtraDef,
    MetricsHandler,
};
use crate::schema::parse_experiments;
use crate::stateful::behavior::EventStore;
use crate::stateful::client::{NimbusServerSettings, SettingsClient, create_client};
use crate::stateful::dbcache::DatabaseCache;
use crate::stateful::enrollment::{
    get_experiment_participation, get_rollout_participation, opt_in_with_branch, opt_out,
    reset_telemetry_identifiers, set_experiment_participation, set_rollout_participation,
    unenroll_for_pref,
};
use crate::stateful::gecko_prefs::{
    GeckoPref, GeckoPrefHandler, GeckoPrefState, GeckoPrefStore, OriginalGeckoPref, PrefBranch,
    PrefEnrollmentData, PrefUnenrollReason,
};
use crate::stateful::matcher::AppContext;
use crate::stateful::persistence::{Database, StoreId, Writer};
use crate::stateful::targeting::{RecordedContext, validate_event_queries};
use crate::stateful::updating::{read_and_remove_pending_experiments, write_pending_experiments};
use crate::strings::fmt_with_map;
#[cfg(test)]
use crate::tests::helpers::{TestGeckoPrefHandler, TestRecordedContext};
use crate::{
    AvailableExperiment, AvailableRandomizationUnits, EnrolledExperiment, EnrollmentStatus,
};
use crate::{Experiment, ExperimentBranch, NimbusError, NimbusTargetingHelper, Result};

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
    pub(crate) install_date: Option<DateTime<Utc>>,
    pub(crate) update_date: Option<DateTime<Utc>>,
    // Application level targeting attributes
    pub(crate) targeting_attributes: TargetingAttributes,
}

impl InternalMutableState {
    pub(crate) fn update_time_to_now(&mut self, now: DateTime<Utc>) {
        self.targeting_attributes
            .update_time_to_now(now, &self.install_date, &self.update_date);
    }
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
    recorded_context: Option<Arc<dyn RecordedContext>>,
    pub(crate) gecko_prefs: Option<Arc<GeckoPrefStore>>,
    metrics_handler: Arc<dyn MetricsHandler>,
}

impl NimbusClient {
    // This constructor *must* not do any kind of I/O since it might be called on the main
    // thread in the gecko Javascript stack, hence the use of OnceCell for the db.
    #[allow(clippy::too_many_arguments)]
    pub fn new<P: Into<PathBuf>>(
        app_context: AppContext,
        recorded_context: Option<Arc<dyn RecordedContext>>,
        coenrolling_feature_ids: Vec<String>,
        db_path: P,
        metrics_handler: Arc<dyn MetricsHandler>,
        gecko_pref_handler: Option<Box<dyn GeckoPrefHandler>>,
        remote_settings_info: Option<NimbusServerSettings>,
    ) -> Result<Self> {
        let settings_client = Mutex::new(create_client(remote_settings_info)?);

        let targeting_attributes: TargetingAttributes = app_context.clone().into();
        let mutable_state = Mutex::new(InternalMutableState {
            available_randomization_units: Default::default(),
            targeting_attributes,
            install_date: Default::default(),
            update_date: Default::default(),
        });

        let mut prefs = None;
        if let Some(handler) = gecko_pref_handler {
            prefs = Some(Arc::new(GeckoPrefStore::new(Arc::new(handler))));
        }

        info!(
            "Initialized NimbusClient with: app_context = {:?}; recorded_context = {:?}",
            app_context,
            recorded_context
                .as_ref()
                .map(|rc| serde_json::Value::Object(rc.to_json()))
                .unwrap_or(serde_json::Value::Null)
        );

        Ok(Self {
            settings_client,
            mutable_state,
            app_context,
            database_cache: Default::default(),
            db_path: db_path.into(),
            coenrolling_feature_ids,
            db: OnceCell::default(),
            event_store: Arc::default(),
            recorded_context,
            gecko_prefs: prefs,
            metrics_handler,
        })
    }

    pub fn with_targeting_attributes(&mut self, targeting_attributes: TargetingAttributes) {
        let mut state = self.mutable_state.lock().unwrap();
        state.targeting_attributes = targeting_attributes;
    }

    pub fn get_targeting_attributes(&self) -> TargetingAttributes {
        let mut state = self.mutable_state.lock().unwrap();
        state.update_time_to_now(Utc::now());
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
        self.read_or_create_nimbus_id(db, writer, state)?;
        self.update_ta_install_dates(db, writer, state)?;
        self.event_store
            .lock()
            .expect("unable to lock event_store mutex")
            .read_from_db(db)?;

        if let Some(recorded_context) = &self.recorded_context {
            let targeting_helper = self.create_targeting_helper_with_context(match serde_json::to_value(
                &state.targeting_attributes,
            ) {
                Ok(v) => v,
                Err(e) => return Err(NimbusError::JSONError("targeting_helper = nimbus::stateful::nimbus_client::NimbusClient::begin_initialize::serde_json::to_value".into(), e.to_string()))
            });
            recorded_context.execute_queries(targeting_helper.as_ref())?;
            state
                .targeting_attributes
                .set_recorded_context(recorded_context.to_json());
        }

        if let Some(gecko_prefs) = &self.gecko_prefs {
            gecko_prefs.initialize()?;
        }

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
        self.database_cache.commit_and_update(
            db,
            writer,
            &coenrolling_ids,
            self.gecko_prefs.clone(),
        )?;
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
        Ok(
            if let Some(s) = self
                .database_cache
                .get_feature_config_variables(&feature_id)?
            {
                self.record_feature_activation_if_needed(&feature_id);
                Some(s)
            } else {
                None
            },
        )
    }

    pub fn get_experiment_branches(&self, slug: String) -> Result<Vec<ExperimentBranch>> {
        self.get_all_experiments()?
            .into_iter()
            .find(|e| e.slug == slug)
            .map(|e| e.branches.into_iter().map(|b| b.into()).collect())
            .ok_or(NimbusError::NoSuchExperiment(slug))
    }

    pub fn get_experiment_participation(&self) -> Result<bool> {
        let db = self.db()?;
        let reader = db.read()?;
        get_experiment_participation(db, &reader)
    }

    pub fn get_rollout_participation(&self) -> Result<bool> {
        let db = self.db()?;
        let reader = db.read()?;
        get_rollout_participation(db, &reader)
    }

    pub fn set_experiment_participation(
        &self,
        user_participating: bool,
    ) -> Result<Vec<EnrollmentChangeEvent>> {
        let db = self.db()?;
        let mut writer = db.write()?;
        let mut state = self.mutable_state.lock().unwrap();
        set_experiment_participation(db, &mut writer, user_participating)?;

        let existing_experiments: Vec<Experiment> =
            db.get_store(StoreId::Experiments).collect_all(&writer)?;
        let events = self.evolve_experiments(db, &mut writer, &mut state, &existing_experiments)?;
        let res = self.end_initialize(db, writer, &mut state);
        self.record_enrollment_status_telemetry(&mut state)?;
        res?;
        Ok(events)
    }

    pub fn set_rollout_participation(
        &self,
        user_participating: bool,
    ) -> Result<Vec<EnrollmentChangeEvent>> {
        let db = self.db()?;
        let mut writer = db.write()?;
        let mut state = self.mutable_state.lock().unwrap();
        set_rollout_participation(db, &mut writer, user_participating)?;

        let existing_experiments: Vec<Experiment> =
            db.get_store(StoreId::Experiments).collect_all(&writer)?;
        let events = self.evolve_experiments(db, &mut writer, &mut state, &existing_experiments)?;
        let res = self.end_initialize(db, writer, &mut state);
        self.record_enrollment_status_telemetry(&mut state)?;
        res?;
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
            .filter(|exp| {
                is_experiment_available(&th, exp, false) == ExperimentAvailable::Available
            })
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
        let result = opt_out(
            db,
            &mut writer,
            &experiment_slug,
            self.gecko_prefs.as_deref(),
        )?;
        let mut state = self.mutable_state.lock().unwrap();
        self.end_initialize(db, writer, &mut state)?;
        Ok(result)
    }

    pub fn fetch_experiments(&self) -> Result<()> {
        if !self.is_fetch_enabled()? {
            return Ok(());
        }
        info!("fetching experiments");
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
        // Only set install_date and update_date with this method if it hasn't been set already.
        // This cuts down on deriving the dates at runtime, but also allows us to use
        // the test methods set_install_date() and set_update_date() to set up
        // scenarios for test.
        if state.install_date.is_none() {
            let installation_date = self.get_installation_date(db, writer)?;
            state.install_date = Some(installation_date);
        }
        if state.update_date.is_none() {
            let update_date = self.get_update_date(db, writer)?;
            state.update_date = Some(update_date);
        }
        state.update_time_to_now(Utc::now());

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

        state
            .targeting_attributes
            .update_enrollments(&prev_enrollments);

        Ok(())
    }

    fn evolve_experiments(
        &self,
        db: &Database,
        writer: &mut Writer,
        state: &mut InternalMutableState,
        experiments: &[Experiment],
    ) -> Result<Vec<EnrollmentChangeEvent>> {
        let mut targeting_helper = NimbusTargetingHelper::with_targeting_attributes(
            &state.targeting_attributes,
            self.event_store.clone(),
            self.gecko_prefs.clone(),
        );
        if let Some(ref recorded_context) = self.recorded_context {
            recorded_context.record();
        }
        let coenrolling_feature_ids = self
            .coenrolling_feature_ids
            .iter()
            .map(|s| s.as_str())
            .collect();
        let mut evolver = EnrollmentsEvolver::new(
            &state.available_randomization_units,
            &mut targeting_helper,
            &coenrolling_feature_ids,
        );
        evolver.evolve_enrollments_in_db(db, writer, experiments, self.gecko_prefs.as_deref())
    }

    pub fn apply_pending_experiments(&self) -> Result<Vec<EnrollmentChangeEvent>> {
        info!("updating experiment list");
        let db = self.db()?;
        let mut writer = db.write()?;

        // We'll get the pending experiments which were stored for us, either by fetch_experiments
        // or by set_experiments_locally.
        let pending_updates = read_and_remove_pending_experiments(db, &mut writer)?;
        let mut state = self.mutable_state.lock().unwrap();
        self.begin_initialize(db, &mut writer, &mut state)?;

        let should_record_enrollment_status = pending_updates.is_some();
        let res = match pending_updates {
            Some(new_experiments) => {
                self.update_ta_active_experiments(db, &writer, &mut state)?;
                // Perform the enrollment calculations if there are pending experiments.
                self.evolve_experiments(db, &mut writer, &mut state, &new_experiments)?
            }
            None => vec![],
        };

        // Finish up any cleanup, e.g. copying from database in to memory.
        let end_init_res = self.end_initialize(db, writer, &mut state);
        if should_record_enrollment_status {
            self.record_enrollment_status_telemetry(&mut state)?;
        }
        end_init_res?;
        Ok(res)
    }

    #[allow(deprecated)] // Bug 1960256 - use of deprecated chrono functions.
    fn get_installation_date(&self, db: &Database, writer: &mut Writer) -> Result<DateTime<Utc>> {
        // we first check our context
        if let Some(context_installation_date) = self.app_context.installation_date {
            let res = DateTime::<Utc>::from_naive_utc_and_offset(
                NaiveDateTime::from_timestamp_opt(context_installation_date / 1_000, 0).unwrap(),
                Utc,
            );
            info!("[Nimbus] Retrieved date from Context: {}", res);
            return Ok(res);
        }
        let store = db.get_store(StoreId::Meta);
        let persisted_installation_date: Option<DateTime<Utc>> =
            store.get(writer, DB_KEY_INSTALLATION_DATE)?;
        Ok(
            if let Some(installation_date) = persisted_installation_date {
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
    pub fn reset_telemetry_identifiers(&self) -> Result<Vec<EnrollmentChangeEvent>> {
        let mut events = vec![];
        let db = self.db()?;
        let mut writer = db.write()?;
        let mut state = self.mutable_state.lock().unwrap();
        // If we have no `nimbus_id` when we can safely assume that there's
        // no other experiment state that needs to be reset.
        let store = db.get_store(StoreId::Meta);
        if store.get::<String, _>(&writer, DB_KEY_NIMBUS_ID)?.is_some() {
            // Each enrollment state now opts out because we don't want to leak information between resets.
            events = reset_telemetry_identifiers(db, &mut writer)?;

            // Remove any stored event counts
            db.clear_event_count_data(&mut writer)?;

            // The `nimbus_id` itself is a unique identifier.
            // N.B. we do this last, as a signal that all data has been reset.
            store.delete(&mut writer, DB_KEY_NIMBUS_ID)?;
            self.end_initialize(db, writer, &mut state)?;
        }

        // (No need to commit `writer` if the above check was false, since we didn't change anything)
        state.available_randomization_units = Default::default();
        state.targeting_attributes.nimbus_id = None;

        Ok(events)
    }

    pub fn nimbus_id(&self) -> Result<Uuid> {
        let db = self.db()?;
        let mut writer = db.write()?;
        let mut state = self.mutable_state.lock().unwrap();
        let uuid = self.read_or_create_nimbus_id(db, &mut writer, &mut state)?;

        // We don't know whether we needed to generate and save the uuid, so
        // we commit just in case - this is hopefully close to a noop in that
        // case!
        writer.commit()?;
        Ok(uuid)
    }

    /// Return the nimbus ID from the database, or create a new one and write it
    /// to the database.
    ///
    /// The internal state will be updated with the nimbus ID.
    fn read_or_create_nimbus_id(
        &self,
        db: &Database,
        writer: &mut Writer,
        state: &mut MutexGuard<'_, InternalMutableState>,
    ) -> Result<Uuid> {
        let store = db.get_store(StoreId::Meta);
        let nimbus_id = match store.get(writer, DB_KEY_NIMBUS_ID)? {
            Some(nimbus_id) => nimbus_id,
            None => {
                let nimbus_id = Uuid::new_v4();
                store.put(writer, DB_KEY_NIMBUS_ID, &nimbus_id)?;
                nimbus_id
            }
        };

        state.available_randomization_units.nimbus_id = Some(nimbus_id.to_string());
        state.targeting_attributes.nimbus_id = Some(nimbus_id.to_string());

        Ok(nimbus_id)
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
        let targeting = match serde_json::to_value(self.get_targeting_attributes()) {
            Ok(v) => v,
            Err(e) => return Err(NimbusError::JSONError("targeting = nimbus::stateful::nimbus_client::NimbusClient::merge_additional_context::serde_json::to_value".into(), e.to_string()))
        };
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
        let helper =
            NimbusTargetingHelper::new(context, self.event_store.clone(), self.gecko_prefs.clone());
        Ok(Arc::new(helper))
    }

    pub fn create_targeting_helper_with_context(
        &self,
        context: Value,
    ) -> Arc<NimbusTargetingHelper> {
        Arc::new(NimbusTargetingHelper::new(
            context,
            self.event_store.clone(),
            self.gecko_prefs.clone(),
        ))
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
        info!("{0: <65}| {1: <30}| {2}", "Slug", "Features", "Branch");
        for exp in &experiments {
            info!(
                "{0: <65}| {1: <30}| {2}",
                &exp.slug,
                &exp.feature_ids.join(", "),
                &exp.branch_slug
            );
        }
        Ok(())
    }

    /// Given a Gecko pref state and a pref unenroll reason, unenroll from an experiment
    pub fn unenroll_for_gecko_pref(
        &self,
        pref_state: GeckoPrefState,
        pref_unenroll_reason: PrefUnenrollReason,
    ) -> Result<Vec<EnrollmentChangeEvent>> {
        if let Some(prefs) = self.gecko_prefs.clone() {
            {
                let mut pref_store_state = prefs.get_mutable_pref_state();
                pref_store_state.update_pref_state(&pref_state);
            }
            let enrollments = self
                .database_cache
                .get_enrollments_for_pref(&pref_state.gecko_pref.pref)?;

            let db = self.db()?;
            let mut writer = db.write()?;

            let mut results = Vec::new();
            for experiment_slug in enrollments.unwrap() {
                let result = unenroll_for_pref(
                    db,
                    &mut writer,
                    &experiment_slug,
                    pref_unenroll_reason,
                    &pref_state.gecko_pref.pref,
                    self.gecko_prefs.as_deref(),
                )?;
                results.push(result);
            }

            let mut state = self.mutable_state.lock().unwrap();
            self.end_initialize(db, writer, &mut state)?;
            return Ok(results.concat());
        }
        Ok(Vec::new())
    }

    pub fn register_previous_gecko_pref_states(
        &self,
        gecko_pref_states: &[GeckoPrefState],
    ) -> Result<()> {
        let all_prev_gecko_pref_states =
            super::gecko_prefs::build_prev_gecko_pref_states(gecko_pref_states);

        let db = self.db()?;
        let mut writer = db.write()?;

        for (experiment_slug, prev_gecko_pref_states) in all_prev_gecko_pref_states {
            Self::add_prev_gecko_pref_state_for_experiment(
                db,
                &mut writer,
                &experiment_slug,
                prev_gecko_pref_states,
            )?;
        }

        let mut state = self.mutable_state.lock().unwrap();
        self.end_initialize(db, writer, &mut state)?;
        Ok(())
    }

    pub(crate) fn add_prev_gecko_pref_state_for_experiment(
        db: &Database,
        writer: &mut Writer,
        experiment_slug: &str,
        prev_gecko_pref_states: Vec<PreviousGeckoPrefState>,
    ) -> Result<()> {
        let enrollments = db.get_store(StoreId::Enrollments);

        if let Ok(Some(existing_enrollment)) =
            enrollments.get::<ExperimentEnrollment, Writer>(writer, experiment_slug)
        {
            // Previous states are only valid on Enrolled experiments
            let updated_states =
                existing_enrollment.on_add_gecko_pref_states(prev_gecko_pref_states);
            enrollments.put(writer, experiment_slug, &updated_states)?;
        }
        Ok(())
    }

    pub fn get_previous_gecko_pref_states(
        &self,
        experiment_slug: String,
    ) -> Result<Option<Vec<PreviousGeckoPrefState>>> {
        let db = self.db()?;
        let reader = db.read()?;

        Ok(db
            .get_store(StoreId::Enrollments)
            .get::<ExperimentEnrollment, _>(&reader, &experiment_slug)?
            .and_then(|enrollment| {
                if let EnrollmentStatus::Enrolled {
                    prev_gecko_pref_states: prev_gecko_pref_state,
                    ..
                } = enrollment.status
                {
                    prev_gecko_pref_state
                } else {
                    None
                }
            }))
    }

    #[cfg(test)]
    pub fn get_recorded_context(&self) -> &&TestRecordedContext {
        self.recorded_context
            .clone()
            .map(|ref recorded_context|
                // SAFETY: The cast to TestRecordedContext is safe because the Rust instance is
                // guaranteed to be a TestRecordedContext instance. TestRecordedContext is the only
                // Rust-implemented version of RecordedContext, and, like this method,  is only
                // used in tests.
                unsafe {
                    std::mem::transmute::<&&dyn RecordedContext, &&TestRecordedContext>(
                        &&**recorded_context,
                    )
                })
            .expect("failed to unwrap RecordedContext object")
    }

    #[cfg(test)]
    pub fn get_gecko_pref_store(&self) -> Arc<Box<TestGeckoPrefHandler>> {
        self.gecko_prefs.clone()
            .clone()
            .map(|ref pref_store|
                // SAFETY: The cast to TestGeckoPrefHandler is safe because the Rust instance is
                // guaranteed to be a TestGeckoPrefHandler instance. TestGeckoPrefHandler is the only
                // Rust-implemented version of GeckoPrefHandler, and, like this method,  is only
                // used in tests.
                unsafe {
                    std::mem::transmute::<Arc<Box<dyn GeckoPrefHandler>>, Arc<Box<TestGeckoPrefHandler>>>(
                        pref_store.clone().handler.clone(),
                    )
                })
            .expect("failed to unwrap GeckoPrefHandler object")
    }
}

impl NimbusClient {
    pub fn set_install_time(&mut self, then: DateTime<Utc>) {
        let mut state = self.mutable_state.lock().unwrap();
        state.install_date = Some(then);
        state.update_time_to_now(Utc::now());
    }

    pub fn set_update_time(&mut self, then: DateTime<Utc>) {
        let mut state = self.mutable_state.lock().unwrap();
        state.update_date = Some(then);
        state.update_time_to_now(Utc::now());
    }
}

impl NimbusClient {
    /// This is only called from `get_feature_config_variables` which is itself is cached with
    /// thread safety in the FeatureHolder.kt and FeatureHolder.swift
    fn record_feature_activation_if_needed(&self, feature_id: &str) {
        if let Ok(Some(f)) = self.database_cache.get_enrollment_by_feature(feature_id)
            && f.branch.is_some()
            && !self.coenrolling_feature_ids.contains(&f.feature_id)
        {
            self.metrics_handler.record_feature_activation(f.into());
        }
    }

    pub fn record_feature_exposure(&self, feature_id: String, slug: Option<String>) {
        let event = if let Some(slug) = slug {
            if let Ok(Some(branch)) = self.database_cache.get_experiment_branch(&slug) {
                Some(FeatureExposureExtraDef {
                    feature_id,
                    branch: Some(branch),
                    slug,
                })
            } else {
                None
            }
        } else if let Ok(Some(f)) = self.database_cache.get_enrollment_by_feature(&feature_id) {
            if f.branch.is_some() {
                Some(f.into())
            } else {
                None
            }
        } else {
            None
        };

        if let Some(event) = event {
            self.metrics_handler.record_feature_exposure(event);
        }
    }

    pub fn record_malformed_feature_config(&self, feature_id: String, part_id: String) {
        let event = if let Ok(Some(f)) = self.database_cache.get_enrollment_by_feature(&feature_id)
        {
            MalformedFeatureConfigExtraDef::from_feature_and_part(f, part_id)
        } else {
            MalformedFeatureConfigExtraDef::new(feature_id, part_id)
        };
        self.metrics_handler.record_malformed_feature_config(event);
    }

    fn record_enrollment_status_telemetry(
        &self,
        state: &mut MutexGuard<InternalMutableState>,
    ) -> Result<()> {
        let targeting_helper = NimbusTargetingHelper::new(
            state.targeting_attributes.clone(),
            self.event_store.clone(),
            self.gecko_prefs.clone(),
        );
        let experiments = self.database_cache.get_experiments()?;
        let experiments = experiments
            .iter()
            .filter(|exp| {
                is_experiment_available(&targeting_helper, exp, true)
                    == ExperimentAvailable::Available
            })
            .map(|exp| &*exp.slug)
            .collect::<HashSet<&str>>();
        self.metrics_handler.record_enrollment_statuses(
            self.database_cache
                .get_enrollments()?
                .into_iter()
                .filter_map(|e| match experiments.contains(&*e.slug) {
                    true => Some(e.into()),
                    false => None,
                })
                .collect(),
        );
        self.metrics_handler.submit_targeting_context();
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

#[cfg(feature = "stateful-uniffi-bindings")]
uniffi::custom_type!(JsonObject, String, {
    remote,
    try_lift: |val| {
        let json: Value = serde_json::from_str(&val)?;

        match json.as_object() {
            Some(obj) => Ok(obj.clone()),
            _ => Err(uniffi::deps::anyhow::anyhow!(
                "Unexpected JSON-non-object in the bagging area"
            )),
        }
    },
    lower: |obj| serde_json::Value::Object(obj).to_string(),
});

#[cfg(feature = "stateful-uniffi-bindings")]
uniffi::custom_type!(PrefValue, String, {
    remote,
    try_lift: |val| {
        let json: Value = serde_json::from_str(&val)?;
        if json.is_string() || json.is_boolean() || (json.is_number() && !json.is_f64()) || json.is_null() {
            Ok(json)
        } else {
            Err(anyhow::anyhow!(format!("Value {} is not a string, boolean, number, or null, or is a float", json)))
        }
    },
    lower: |val| {
        val.to_string()
    }
});

#[cfg(feature = "stateful-uniffi-bindings")]
uniffi::include_scaffolding!("nimbus");
