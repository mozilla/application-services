/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Our storage abstraction, currently backed by Rkv.

use crate::error::{debug, info, warn, NimbusError, Result};
// This uses the lmdb backend for rkv, which is unstable.
// We use it for now since glean didn't seem to have trouble with it (although
// it must be noted that the rkv documentation explicitly says "To use rkv in
// production/release environments at Mozilla, you may do so with the "SafeMode"
// backend", so we really should get more guidance here.)
use crate::enrollment::ExperimentEnrollment;
use crate::Experiment;
use core::iter::Iterator;
use rkv::{StoreError, StoreOptions};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

// We use an incrementing integer to manage database migrations.
// If you need to make a backwards-incompatible change to the data schema,
// increment `DB_VERSION` and implement some migration logic in `maybe_upgrade`.
//
// ⚠️ Warning : Altering the type of `DB_VERSION` would itself require a DB migration. ⚠️
pub(crate) const DB_KEY_DB_VERSION: &str = "db_version";
pub(crate) const DB_VERSION: u16 = 2;
const RKV_MAX_DBS: u32 = 6;

// Inspired by Glean - use a feature to choose between the backends.
// Select the LMDB-powered storage backend when the feature is not activated.
#[cfg(not(feature = "rkv-safe-mode"))]
mod backend {
    use rkv::backend::{
        Lmdb, LmdbDatabase, LmdbEnvironment, LmdbRoCursor, LmdbRoTransaction, LmdbRwTransaction,
    };
    use std::path::Path;

    use super::RKV_MAX_DBS;

    pub type Rkv = rkv::Rkv<LmdbEnvironment>;
    pub type RkvSingleStore = rkv::SingleStore<LmdbDatabase>;
    pub type Reader<'t> = rkv::Reader<LmdbRoTransaction<'t>>;
    pub type Writer<'t> = rkv::Writer<LmdbRwTransaction<'t>>;
    pub trait Readable<'r>:
        rkv::Readable<'r, Database = LmdbDatabase, RoCursor = LmdbRoCursor<'r>>
    {
    }
    impl<'r, T: rkv::Readable<'r, Database = LmdbDatabase, RoCursor = LmdbRoCursor<'r>>>
        Readable<'r> for T
    {
    }

    pub fn rkv_new(path: &Path) -> Result<Rkv, rkv::StoreError> {
        Rkv::with_capacity::<Lmdb>(path, RKV_MAX_DBS)
    }
}

// Select the "safe mode" storage backend when the feature is activated.
#[cfg(feature = "rkv-safe-mode")]
mod backend {
    use rkv::backend::{
        SafeMode, SafeModeDatabase, SafeModeEnvironment, SafeModeRoCursor, SafeModeRoTransaction,
        SafeModeRwTransaction,
    };
    use std::path::Path;

    use super::RKV_MAX_DBS;

    pub type Rkv = rkv::Rkv<SafeModeEnvironment>;
    pub type RkvSingleStore = rkv::SingleStore<SafeModeDatabase>;
    pub type Reader<'t> = rkv::Reader<SafeModeRoTransaction<'t>>;
    pub type Writer<'t> = rkv::Writer<SafeModeRwTransaction<'t>>;
    pub trait Readable<'r>:
        rkv::Readable<'r, Database = SafeModeDatabase, RoCursor = SafeModeRoCursor<'r>>
    {
    }
    impl<
            'r,
            T: rkv::Readable<'r, Database = SafeModeDatabase, RoCursor = SafeModeRoCursor<'r>>,
        > Readable<'r> for T
    {
    }

    pub fn rkv_new(path: &Path) -> Result<Rkv, rkv::StoreError> {
        Rkv::with_capacity::<SafeMode>(path, RKV_MAX_DBS)
    }
}

use backend::*;
pub use backend::{Readable, Writer};

/// Enumeration of the different stores within our database.
///
/// Our rkv database contains a number of different "stores", and the items
/// in each store correspond to a particular type of object at the Rust level.
pub enum StoreId {
    /// Store containing the set of known experiments, as read from the server.
    ///
    /// Keys in the `Experiments` store are experiment identifier slugs, and their
    /// corresponding values are  serialized instances of the [`Experiment`] struct
    /// representing the last known state of that experiment.
    Experiments,
    /// Store containing the set of known experiment enrollments.
    ///
    /// Keys in the `Enrollments` store are experiment identifier slugs, and their
    /// corresponding values are serialized instances of the [`ExperimentEnrollment`]
    /// struct representing the current state of this client's enrollment (or not)
    /// in that experiment.
    Enrollments,
    /// Store containing miscellaneous metadata about this client instance.
    ///
    /// Keys in the `Meta` store are string constants, and their corresponding values
    /// are serialized items whose type depends on the constant. Known constraints
    /// include:
    ///   * "db_version":   u16, the version number of the most revent migration
    ///     applied to this database.
    ///   * "nimbus-id":    String, the randomly-generated identifier for the
    ///     current client instance.
    ///   * "user-opt-in":  bool, whether the user has explicitly opted in or out
    ///     of participating in experiments.
    ///   * "installation-date": a UTC DateTime string, defining the date the consuming app was
    ///     installed
    ///   * "update-date": a UTC DateTime string, defining the date the consuming app was
    ///     last updated
    ///   * "app-version": String, the version of the app last persisted
    Meta,
    /// Store containing pending updates to experiment data.
    ///
    /// The `Updates` store contains a single key "pending-experiment-updates", whose
    /// corresponding value is a serialized `Vec<Experiment>` of new experiment data
    /// that has been received from the server but not yet processed by the application.
    Updates,
    /// Store containing collected counts of behavior events for targeting purposes.
    ///
    /// Keys in the `EventCounts` store are strings representing the identifier for
    /// the event and their corresponding values represent a serialized instance of a
    /// [`MultiIntervalCounter`] struct that contains a set of configurations and data
    /// for the different time periods that the data will be aggregated on.
    EventCounts,
}

/// A wrapper for an Rkv store. Implemented to allow any value which supports
/// serde to be used.
pub struct SingleStore {
    store: RkvSingleStore,
}

impl SingleStore {
    pub fn new(store: RkvSingleStore) -> Self {
        SingleStore { store }
    }

    pub fn put<T: serde::Serialize + for<'de> serde::Deserialize<'de>>(
        &self,
        writer: &mut Writer,
        key: &str,
        persisted_data: &T,
    ) -> Result<()> {
        let persisted_json = match serde_json::to_string(persisted_data) {
            Ok(v) => v,
            Err(e) => return Err(NimbusError::JSONError("persisted_json = nimbus::stateful::persistence::SingleStore::put::serde_json::to_string".into(), e.to_string()))
        };
        self.store
            .put(writer, key, &rkv::Value::Json(&persisted_json))?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn delete(&self, writer: &mut Writer, key: &str) -> Result<()> {
        self.store.delete(writer, key)?;
        Ok(())
    }

    pub fn clear(&self, writer: &mut Writer) -> Result<()> {
        self.store.clear(writer)?;
        Ok(())
    }

    // Some "get" functions that cooperate with transactions (ie, so we can
    // get what we've written to the transaction before it's committed).
    // It's unfortunate that these are duplicated with the DB itself, but the
    // traits used by rkv make this tricky.
    pub fn get<'r, T, R>(&self, reader: &'r R, key: &str) -> Result<Option<T>>
    where
        R: Readable<'r>,
        T: serde::Serialize + for<'de> serde::Deserialize<'de>,
    {
        let persisted_data = self.store.get(reader, key)?;
        match persisted_data {
            Some(data) => {
                if let rkv::Value::Json(data) = data {
                    Ok(Some(match serde_json::from_str::<T>(data) {
                        Ok(v) => v,
                        Err(e) => return Err(NimbusError::JSONError("match persisted_data nimbus::stateful::persistence::SingleStore::get::serde_json::from_str".into(), e.to_string()))
                    }))
                } else {
                    Err(NimbusError::InvalidPersistedData)
                }
            }
            None => Ok(None),
        }
    }

    /// Fork of collect_all that simply drops records that fail to read
    /// rather than simply returning an error up the stack.  This likely
    /// wants to be just a parameter to collect_all, but for now....
    pub fn try_collect_all<'r, T, R>(&self, reader: &'r R) -> Result<Vec<T>>
    where
        R: Readable<'r>,
        T: serde::Serialize + for<'de> serde::Deserialize<'de>,
    {
        let mut result = Vec::new();
        let mut iter = self.store.iter_start(reader)?;
        while let Some(Ok((_, data))) = iter.next() {
            if let rkv::Value::Json(data) = data {
                let unserialized = serde_json::from_str::<T>(data);
                match unserialized {
                    Ok(value) => result.push(value),
                    Err(e) => {
                        // If there is an error, we won't push this onto the
                        // result Vec, but we won't blow up the entire
                        // deserialization either.
                        warn!(
                            "try_collect_all: discarded a record while deserializing with: {:?}",
                            e
                        );
                        warn!(
                            "try_collect_all:   data that failed to deserialize: {:?}",
                            data
                        );
                    }
                };
            }
        }
        Ok(result)
    }

    pub fn collect_all<'r, T, R>(&self, reader: &'r R) -> Result<Vec<T>>
    where
        R: Readable<'r>,
        T: serde::Serialize + for<'de> serde::Deserialize<'de>,
    {
        let mut result = Vec::new();
        let mut iter = self.store.iter_start(reader)?;
        while let Some(Ok((_, data))) = iter.next() {
            if let rkv::Value::Json(data) = data {
                result.push(match serde_json::from_str::<T>(data) {
                    Ok(v) => v,
                    Err(e) => return Err(NimbusError::JSONError("rkv::Value::Json(data) nimbus::stateful::persistence::SingleStore::collect_all::serde_json::from_str".into(), e.to_string()))
                });
            }
        }
        Ok(result)
    }
}

pub struct SingleStoreDatabase {
    rkv: Rkv,
    pub(crate) store: SingleStore,
}

impl SingleStoreDatabase {
    /// Function used to obtain a "reader" which is used for read-only transactions.
    pub fn read(&self) -> Result<Reader> {
        Ok(self.rkv.read()?)
    }

    /// Function used to obtain a "writer" which is used for transactions.
    /// The `writer.commit();` must be called to commit data added via the
    /// writer.
    pub fn write(&self) -> Result<Writer> {
        Ok(self.rkv.write()?)
    }

    /// Function used to obtain values from the internal store.
    pub fn get<'r, T, R>(&self, reader: &'r R, key: &str) -> Result<Option<T>>
    where
        R: Readable<'r>,
        T: serde::Serialize + for<'de> serde::Deserialize<'de>,
    {
        self.store.get(reader, key)
    }
}

/// Database used to access persisted data
/// This an abstraction around an Rkv database
/// An instance on this database is created each time the component is loaded
/// if there is persisted data, the `get` functions should retrieve it
pub struct Database {
    rkv: Rkv,
    meta_store: SingleStore,
    experiment_store: SingleStore,
    enrollment_store: SingleStore,
    updates_store: SingleStore,
    event_count_store: SingleStore,
}

impl Database {
    /// Main constructor for a database
    /// Initiates the Rkv database to be used to retrieve persisted data
    /// # Arguments
    /// - `path`: A path to the persisted data, this is provided by the consuming application
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let rkv = Self::open_rkv(path)?;
        let meta_store = rkv.open_single("meta", StoreOptions::create())?;
        let experiment_store = rkv.open_single("experiments", StoreOptions::create())?;
        let enrollment_store = rkv.open_single("enrollments", StoreOptions::create())?;
        let updates_store = rkv.open_single("updates", StoreOptions::create())?;
        let event_count_store = rkv.open_single("event_counts", StoreOptions::create())?;
        let db = Self {
            rkv,
            meta_store: SingleStore::new(meta_store),
            experiment_store: SingleStore::new(experiment_store),
            enrollment_store: SingleStore::new(enrollment_store),
            updates_store: SingleStore::new(updates_store),
            event_count_store: SingleStore::new(event_count_store),
        };
        db.maybe_upgrade()?;
        Ok(db)
    }

    pub fn open_single<P: AsRef<Path>>(path: P, store_id: StoreId) -> Result<SingleStoreDatabase> {
        let rkv = Self::open_rkv(path)?;
        let store = SingleStore::new(match store_id {
            StoreId::Experiments => rkv.open_single("experiments", StoreOptions::create())?,
            StoreId::Enrollments => rkv.open_single("enrollments", StoreOptions::create())?,
            StoreId::Meta => rkv.open_single("meta", StoreOptions::create())?,
            StoreId::Updates => rkv.open_single("updates", StoreOptions::create())?,
            StoreId::EventCounts => rkv.open_single("event_counts", StoreOptions::create())?,
        });
        Ok(SingleStoreDatabase { rkv, store })
    }

    fn maybe_upgrade(&self) -> Result<()> {
        debug!("entered maybe upgrade");
        let mut writer = self.rkv.write()?;
        let db_version = self.meta_store.get::<u16, _>(&writer, DB_KEY_DB_VERSION)?;
        match db_version {
            Some(DB_VERSION) => {
                // Already at the current version, no migration required.
                info!("Already at version {}, no upgrade needed", DB_VERSION);
                return Ok(());
            }
            Some(1) => {
                info!("Migrating database from v1 to v2");
                match self.migrate_v1_to_v2(&mut writer) {
                    Ok(_) => (),
                    Err(e) => {
                        // The idea here is that it's better to leave an
                        // individual install with a clean empty database
                        // than in an unknown inconsistent state, because it
                        // allows them to start participating in experiments
                        // again, rather than potentially repeating the upgrade
                        // over and over at each embedding client restart.
                        error_support::report_error!(
                            "nimbus-database-migration",
                            "Error migrating database v1 to v2: {:?}.  Wiping experiments and enrollments",
                            e
                        );
                        self.clear_experiments_and_enrollments(&mut writer)?;
                    }
                };
            }
            None => {
                info!("maybe_upgrade: no version number; wiping most stores");
                // The "first" version of the database (= no version number) had un-migratable data
                // for experiments and enrollments, start anew.
                // XXX: We can most likely remove this behaviour once enough time has passed,
                // since nimbus wasn't really shipped to production at the time anyway.
                self.clear_experiments_and_enrollments(&mut writer)?;
            }
            _ => {
                error_support::report_error!(
                    "nimbus-unknown-database-version",
                    "Unknown database version. Wiping all stores."
                );
                self.clear_experiments_and_enrollments(&mut writer)?;
                self.meta_store.clear(&mut writer)?;
            }
        }
        // It is safe to clear the update store (i.e. the pending experiments) on all schema upgrades
        // as it will be re-filled from the server on the next `fetch_experiments()`.
        // The current contents of the update store may cause experiments to not load, or worse,
        // accidentally unenroll.
        self.updates_store.clear(&mut writer)?;
        self.meta_store
            .put(&mut writer, DB_KEY_DB_VERSION, &DB_VERSION)?;
        writer.commit()?;
        debug!("maybe_upgrade: transaction committed");
        Ok(())
    }

    pub(crate) fn clear_experiments_and_enrollments(
        &self,
        writer: &mut Writer,
    ) -> Result<(), NimbusError> {
        self.experiment_store.clear(writer)?;
        self.enrollment_store.clear(writer)?;
        Ok(())
    }

    pub(crate) fn clear_event_count_data(&self, writer: &mut Writer) -> Result<(), NimbusError> {
        self.event_count_store.clear(writer)?;
        Ok(())
    }

    /// Migrates a v1 database to v2
    ///
    /// Note that any Err returns from this function (including stuff
    /// propagated up via the ? operator) will cause maybe_update (our caller)
    /// to assume that this is unrecoverable and wipe the database, removing
    /// people from any existing enrollments and blowing away their experiment
    /// history, so that they don't get left in an inconsistent state.
    fn migrate_v1_to_v2(&self, writer: &mut Writer) -> Result<()> {
        info!("Upgrading from version 1 to version 2");

        // use try_collect_all to read everything except records that serde
        // returns deserialization errors on.  Some logging of those errors
        // happens, but it's not ideal.
        let reader = self.read()?;

        // XXX write a test to verify that we don't need to gc any
        // enrollments that don't have experiments because the experiments
        // were discarded either during try_collect_all (these wouldn't have been
        // detected during the filtering phase) or during the filtering phase
        // itself.  The test needs to run evolve_experiments, as that should
        // correctly drop any orphans, even if the migrators aren't perfect.

        let enrollments: Vec<ExperimentEnrollment> =
            self.enrollment_store.try_collect_all(&reader)?;
        let experiments: Vec<Experiment> = self.experiment_store.try_collect_all(&reader)?;

        // figure out which experiments have records that need to be dropped
        // and log that we're going to drop them and why
        let empty_string = "".to_string();
        let slugs_with_experiment_issues: HashSet<String> = experiments
            .iter()
            .filter_map(
                    |e| {
                let branch_with_empty_feature_ids =
                    e.branches.iter().find(|b| b.feature.is_none() || b.feature.as_ref().unwrap().feature_id.is_empty());
                if branch_with_empty_feature_ids.is_some() {
                    warn!("{:?} experiment has branch missing a feature prop; experiment & enrollment will be discarded", &e.slug);
                    Some(e.slug.to_owned())
                } else if e.feature_ids.is_empty() || e.feature_ids.contains(&empty_string) {
                    warn!("{:?} experiment has invalid feature_ids array; experiment & enrollment will be discarded", &e.slug);
                    Some(e.slug.to_owned())
                } else {
                    None
                }
            })
            .collect();
        let slugs_to_discard: HashSet<_> = slugs_with_experiment_issues;

        // filter out experiments to be dropped
        let updated_experiments: Vec<Experiment> = experiments
            .into_iter()
            .filter(|e| !slugs_to_discard.contains(&e.slug))
            .collect();
        debug!("updated experiments = {:?}", updated_experiments);

        // filter out enrollments to be dropped
        let updated_enrollments: Vec<ExperimentEnrollment> = enrollments
            .into_iter()
            .filter(|e| !slugs_to_discard.contains(&e.slug))
            .collect();
        debug!("updated enrollments = {:?}", updated_enrollments);

        // rewrite both stores
        self.experiment_store.clear(writer)?;
        for experiment in updated_experiments {
            self.experiment_store
                .put(writer, &experiment.slug, &experiment)?;
        }

        self.enrollment_store.clear(writer)?;
        for enrollment in updated_enrollments {
            self.enrollment_store
                .put(writer, &enrollment.slug, &enrollment)?;
        }
        debug!("exiting migrate_v1_to_v2");

        Ok(())
    }

    /// Gets a Store object, which used with the writer returned by
    /// `self.write()` to update the database in a transaction.
    pub fn get_store(&self, store_id: StoreId) -> &SingleStore {
        match store_id {
            StoreId::Meta => &self.meta_store,
            StoreId::Experiments => &self.experiment_store,
            StoreId::Enrollments => &self.enrollment_store,
            StoreId::Updates => &self.updates_store,
            StoreId::EventCounts => &self.event_count_store,
        }
    }

    pub fn open_rkv<P: AsRef<Path>>(path: P) -> Result<Rkv> {
        let path = std::path::Path::new(path.as_ref()).join("db");
        debug!("open_rkv: path =  {:?}", path.display());
        fs::create_dir_all(&path)?;
        let rkv = match rkv_new(&path) {
            Ok(rkv) => Ok(rkv),
            Err(rkv_error) => {
                match rkv_error {
                    // For some errors we just delete the DB and start again.
                    StoreError::DatabaseCorrupted | StoreError::FileInvalid => {
                        // On one hand this seems a little dangerous, but on
                        // the other hand avoids us knowing about the
                        // underlying implementation (ie, how do we know what
                        // files might exist in all cases?)
                        warn!(
                            "Database at '{}' appears corrupt - removing and recreating",
                            path.display()
                        );
                        fs::remove_dir_all(&path)?;
                        fs::create_dir_all(&path)?;
                        // TODO: Once we have glean integration we want to
                        // record telemetry here.
                        rkv_new(&path)
                    }
                    // All other errors are fatal.
                    _ => Err(rkv_error),
                }
            }
        }?;
        debug!("Database initialized");
        Ok(rkv)
    }

    /// Function used to obtain a "reader" which is used for read-only transactions.
    pub fn read(&self) -> Result<Reader> {
        Ok(self.rkv.read()?)
    }

    /// Function used to obtain a "writer" which is used for transactions.
    /// The `writer.commit();` must be called to commit data added via the
    /// writer.
    pub fn write(&self) -> Result<Writer> {
        Ok(self.rkv.write()?)
    }

    /// Function used to retrieve persisted data outside of a transaction.
    /// It allows retrieval of any serializable and deserializable data
    /// Currently only supports JSON data
    // Only available for tests; product code should always be using transactions.
    ///
    /// # Arguments
    /// - `key`: A key for the data stored in the underlying database
    #[cfg(test)]
    pub fn get<T: serde::Serialize + for<'de> serde::Deserialize<'de>>(
        &self,
        store_id: StoreId,
        key: &str,
    ) -> Result<Option<T>> {
        let reader = self.rkv.read()?;
        let persisted_data = self.get_store(store_id).store.get(&reader, key)?;
        match persisted_data {
            Some(data) => {
                if let rkv::Value::Json(data) = data {
                    Ok(Some(match serde_json::from_str::<T>(data) {
                        Ok(v) => v,
                        Err(e) => return Err(NimbusError::JSONError("rkv::Value::Json(data) nimbus::stateful::persistence::Database::get::serde_json::from_str".into(), e.to_string()))
                    }))
                } else {
                    Err(NimbusError::InvalidPersistedData)
                }
            }
            None => Ok(None),
        }
    }

    // Function for collecting all items in a store outside of a transaction.
    // Only available for tests; product code should always be using transactions.
    // Iters are a bit tricky - would be nice to make them generic, but this will
    // do for our use-case.
    #[cfg(test)]
    pub fn collect_all<T: serde::Serialize + for<'de> serde::Deserialize<'de>>(
        &self,
        store_id: StoreId,
    ) -> Result<Vec<T>> {
        let mut result = Vec::new();
        let reader = self.rkv.read()?;
        let mut iter = self.get_store(store_id).store.iter_start(&reader)?;
        while let Some(Ok((_, data))) = iter.next() {
            if let rkv::Value::Json(data) = data {
                result.push(match serde_json::from_str::<T>(data) {
                    Ok(v) => v,
                    Err(e) => return Err(NimbusError::JSONError("rkv::Value::Json(data) nimbus::stateful::persistence::Database::collect_all::serde_json::from_str".into(), e.to_string()))
                });
            }
        }
        Ok(result)
    }
}
