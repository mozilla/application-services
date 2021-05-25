/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Our storage abstraction, currently backed by Rkv.

use crate::error::{NimbusError, Result};
// This uses the lmdb backend for rkv, which is unstable.
// We use it for now since glean didn't seem to have trouble with it (although
// it must be noted that the rkv documentation explicitly says "To use rkv in
// production/release environments at Mozilla, you may do so with the "SafeMode"
// backend", so we really should get more guidance here.)
use crate::enrollment::{EnrollmentStatus, ExperimentEnrollment};
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
const DB_KEY_DB_VERSION: &str = "db_version";
const DB_VERSION: u16 = 2;

// Inspired by Glean - use a feature to choose between the backends.
// Select the LMDB-powered storage backend when the feature is not activated.
#[cfg(not(feature = "rkv-safe-mode"))]
mod backend {
    use rkv::backend::{
        Lmdb, LmdbDatabase, LmdbEnvironment, LmdbRoCursor, LmdbRoTransaction, LmdbRwTransaction,
    };
    use std::path::Path;

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
        Rkv::new::<Lmdb>(path)
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
        Rkv::new::<SafeMode>(path)
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
    /// are serialized items whose type depends on the constant. Known constaints
    /// include:
    ///   * "db_version":   u16, the version number of the most revent migration
    ///                     applied to this database.
    ///   * "nimbus-id":    String, the randomly-generated identifier for the
    ///                     current client instance.
    ///   * "user-opt-in":  bool, whether the user has explicitly opted in or out
    ///                     of participating in experiments.
    Meta,
    /// Store containing pending updates to experiment data.
    ///
    /// The `Updates` store contains a single key "pending-experiment-updates", whose
    /// corresponding value is a serialized `Vec<Experiment>` of new experiment data
    /// that has been received from the server but not yet processed by the application.
    Updates,
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
        mut writer: &mut Writer,
        key: &str,
        persisted_data: &T,
    ) -> Result<()> {
        let persisted_json = serde_json::to_string(persisted_data)?;
        self.store
            .put(&mut writer, key, &rkv::Value::Json(&persisted_json))?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn delete(&self, mut writer: &mut Writer, key: &str) -> Result<()> {
        self.store.delete(&mut writer, key)?;
        Ok(())
    }

    pub fn clear(&self, mut writer: &mut Writer) -> Result<()> {
        self.store.clear(&mut writer)?;
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
                    Ok(Some(serde_json::from_str::<T>(data)?))
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
                let unserialized = serde_json::from_str::<T>(&data);
                match unserialized {
                    Ok(value) => result.push(value),
                    Err(e) => {
                        // If there is an error, we won't push this onto the
                        // result Vec, but we won't blow up the entire
                        // deserialization either.
                        log::warn!(
                            "try_collect_all: discarded a record while deserializing with: {:?}",
                            e
                        );
                        log::warn!(
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
                result.push(serde_json::from_str::<T>(&data)?);
            }
        }
        Ok(result)
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
}

impl Database {
    /// Main constructor for a database
    /// Initiates the Rkv database to be used to retreive persisted data
    /// # Arguments
    /// - `path`: A path to the persisted data, this is provided by the consuming application
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let rkv = Self::open_rkv(path)?;
        let meta_store = rkv.open_single("meta", StoreOptions::create())?;
        let experiment_store = rkv.open_single("experiments", StoreOptions::create())?;
        let enrollment_store = rkv.open_single("enrollments", StoreOptions::create())?;
        let updates_store = rkv.open_single("updates", StoreOptions::create())?;
        let db = Self {
            rkv,
            meta_store: SingleStore::new(meta_store),
            experiment_store: SingleStore::new(experiment_store),
            enrollment_store: SingleStore::new(enrollment_store),
            updates_store: SingleStore::new(updates_store),
        };
        db.maybe_upgrade()?;
        Ok(db)
    }

    fn maybe_upgrade(&self) -> Result<()> {
        log::debug!("entered maybe upgrade");
        let mut writer = self.rkv.write()?;
        let db_version = self.meta_store.get::<u16, _>(&writer, DB_KEY_DB_VERSION)?;
        match db_version {
            Some(DB_VERSION) => {
                // Already at the current version, no migration required.
                return Ok(());
            }
            Some(1) => {
                match self.migrate_v1_to_v2(&mut writer) {
                    Ok(_) => (),
                    Err(e) => {
                        // The idea here is that it's better to leave an
                        // individual install with a clean empty database
                        // than in an unknown inconsistent state, because it
                        // allows them to start participating in experiments
                        // again, rather than potentially repeating the upgrade
                        // over and over at each embedding client restart.
                        log::error!(
                            "Error migrating database v1 to v2: {:?}.  Wiping experiments and enrollments",
                            e
                        );
                        self.clear_experiments_and_enrollments(&mut writer)?;
                    }
                };
            }
            None => {
                // The "first" version of the database (= no version number) had un-migratable data
                // for experiments and enrollments, start anew.
                // XXX: We can most likely remove this behaviour once enough time has passed,
                // since nimbus wasn't really shipped to production at the time anyway.
                self.clear_experiments_and_enrollments(&mut writer)?;
            }
            _ => {
                log::error!("Unknown database version. Wiping everything.");
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
        log::debug!("transaction commited");
        Ok(())
    }

    fn clear_experiments_and_enrollments(&self, writer: &mut Writer) -> Result<(), NimbusError> {
        self.experiment_store.clear(writer)?;
        self.enrollment_store.clear(writer)?;
        Ok(())
    }

    /// Migrates a v1 database to v2
    ///
    /// Note that any Err returns from this function (including stuff
    /// propagated up via the ? operator) will cause maybe_update (our caller)
    /// to assume that this is unrecoverable and wipe the database, removing
    /// people from any existing enrollments and blowing away their experiment
    /// history, so that they don't get left in an inconsistent state.
    fn migrate_v1_to_v2(&self, mut writer: &mut Writer) -> Result<()> {
        log::info!("Upgrading from version 1 to version 2");

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

        // figure out which enrollments have records that need to be dropped
        // and log that we're going to drop them and why
        let slugs_without_enrollment_feature_ids: HashSet<String> = enrollments
            .iter()
            .filter_map(
                    |e| {
                if matches!(e.status, EnrollmentStatus::Enrolled {ref feature_id, ..} if feature_id.is_empty()) {
                    log::warn!("Enrollment for {:?} missing feature_ids; experiment & enrollment will be discarded", &e.slug);
                    Some(e.slug.to_owned())
                } else {
                    None
                }
            })
            .collect();

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
                    log::warn!("{:?} experiment has branch missing a feature prop; experiment & enrollment will be discarded", &e.slug);
                    Some(e.slug.to_owned())
                } else if e.feature_ids.is_empty() || e.feature_ids.contains(&empty_string) {
                    log::warn!("{:?} experiment has invalid feature_ids array; experiment & enrollment will be discarded", &e.slug);
                    Some(e.slug.to_owned())
                } else {
                    None
                }
            })
            .collect();
        let slugs_to_discard: HashSet<_> = slugs_without_enrollment_feature_ids
            .union(&slugs_with_experiment_issues)
            .collect();

        // filter out experiments to be dropped
        let updated_experiments: Vec<Experiment> = experiments
            .into_iter()
            .filter(|e| !slugs_to_discard.contains(&e.slug))
            .collect();
        log::debug!("updated experiments = {:?}", updated_experiments);

        // filter out enrollments to be dropped
        let updated_enrollments: Vec<ExperimentEnrollment> = enrollments
            .into_iter()
            .filter(|e| !slugs_to_discard.contains(&e.slug))
            .collect();
        log::debug!("updated enrollments = {:?}", updated_enrollments);

        // rewrite both stores
        self.experiment_store.clear(&mut writer)?;
        for experiment in updated_experiments {
            self.experiment_store
                .put(&mut writer, &experiment.slug, &experiment)?;
        }

        self.enrollment_store.clear(&mut writer)?;
        for enrollment in updated_enrollments {
            self.enrollment_store
                .put(&mut writer, &enrollment.slug, &enrollment)?;
        }
        log::debug!("exiting migrate_v1_to_v2");

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
        }
    }

    pub fn open_rkv<P: AsRef<Path>>(path: P) -> Result<Rkv> {
        let path = std::path::Path::new(path.as_ref()).join("db");
        log::debug!("Database path: {:?}", path.display());
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
                        log::warn!(
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
        log::debug!("Database initialized");
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
                    Ok(Some(serde_json::from_str::<T>(data)?))
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
                result.push(serde_json::from_str::<T>(&data)?);
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;
    use tempdir::TempDir;

    #[test]
    fn test_db_upgrade_no_version() -> Result<()> {
        let path = "test_upgrade_1";
        let tmp_dir = TempDir::new(path)?;

        let rkv = Database::open_rkv(&tmp_dir)?;
        let _meta_store = rkv.open_single("meta", StoreOptions::create())?;
        let experiment_store =
            SingleStore::new(rkv.open_single("experiments", StoreOptions::create())?);
        let enrollment_store =
            SingleStore::new(rkv.open_single("enrollments", StoreOptions::create())?);
        let mut writer = rkv.write()?;
        enrollment_store.put(&mut writer, "foo", &"bar".to_owned())?;
        experiment_store.put(&mut writer, "bobo", &"tron".to_owned())?;
        writer.commit()?;

        let db = Database::new(&tmp_dir)?;
        assert_eq!(db.get(StoreId::Meta, DB_KEY_DB_VERSION)?, Some(DB_VERSION));
        assert!(db.collect_all::<String>(StoreId::Enrollments)?.is_empty());
        assert!(db.collect_all::<String>(StoreId::Experiments)?.is_empty());

        Ok(())
    }

    #[test]
    fn test_db_upgrade_unknown_version() -> Result<()> {
        let path = "test_upgrade_unknown";
        let tmp_dir = TempDir::new(path)?;

        let rkv = Database::open_rkv(&tmp_dir)?;
        let meta_store = SingleStore::new(rkv.open_single("meta", StoreOptions::create())?);
        let experiment_store =
            SingleStore::new(rkv.open_single("experiments", StoreOptions::create())?);
        let enrollment_store =
            SingleStore::new(rkv.open_single("enrollments", StoreOptions::create())?);
        let mut writer = rkv.write()?;
        meta_store.put(&mut writer, DB_KEY_DB_VERSION, &u16::MAX)?;
        enrollment_store.put(&mut writer, "foo", &"bar".to_owned())?;
        experiment_store.put(&mut writer, "bobo", &"tron".to_owned())?;
        writer.commit()?;
        let db = Database::new(&tmp_dir)?;
        assert_eq!(db.get(StoreId::Meta, DB_KEY_DB_VERSION)?, Some(DB_VERSION));
        assert!(db.collect_all::<String>(StoreId::Enrollments)?.is_empty());
        assert!(db.collect_all::<String>(StoreId::Experiments)?.is_empty());

        Ok(())
    }

    #[test]
    fn test_corrupt_db() -> Result<()> {
        let path = "test_corrupt_db";
        let tmp_dir = TempDir::new(path)?;

        let db_dir = tmp_dir.path().join("db");
        fs::create_dir(db_dir.clone())?;

        // The database filename differs depending on the rkv mode.
        #[cfg(feature = "rkv-safe-mode")]
        let db_file = db_dir.join("data.safe.bin");
        #[cfg(not(feature = "rkv-safe-mode"))]
        let db_file = db_dir.join("data.mdb");

        let garbage = b"Not a database!";
        let garbage_len = garbage.len() as u64;
        fs::write(&db_file, garbage)?;
        assert_eq!(fs::metadata(&db_file)?.len(), garbage_len);
        // Opening the DB should delete the corrupt file and replace it.
        Database::new(&tmp_dir)?;
        // Old contents should be removed and replaced with actual data.
        assert_ne!(fs::metadata(&db_file)?.len(), garbage_len);
        Ok(())
    }

    // XXX secure-gold has some fields. Ideally, we would also have an
    // experiment with all current fields set, and another with almost no
    // optional fields set
    fn db_v1_experiments_with_non_empty_features() -> Vec<serde_json::Value> {
        vec![
            json!({
                "schemaVersion": "1.0.0",
                "slug": "secure-gold", // change when copy/pasting to make experiments
                "endDate": null,
                "featureIds": ["abc"], // change when copy/pasting to make experiments
                "branches":[
                    {
                        "slug": "control",
                        "ratio": 1,
                        "feature": {
                            "featureId": "abc", // change when copy/pasting to make experiments
                            "enabled": false,
                            "value": {"color": "green"}
                        }
                    },
                    {
                        "slug": "treatment",
                        "ratio":1,
                        "feature": {
                            "featureId": "abc", // change when copy/pasting to make experiments
                            "enabled": true,
                            "value": {}
                        }
                    }
                ],
                "channel": "nightly",
                "probeSets":[],
                "startDate":null,
                "appName": "fenix",
                "appId": "org.mozilla.fenix",
                "bucketConfig":{
                    // Setup to enroll everyone by default.
                    "count":10_000,
                    "start":0,
                    "total":10_000,
                    "namespace":"secure-gold", // change when copy/pasting to make experiments
                    "randomizationUnit":"nimbus_id"
                },
                "userFacingName":"Diagnostic test experiment",
                "referenceBranch":"control",
                "isEnrollmentPaused":false,
                "proposedDuration": 21,
                "proposedEnrollment":7,
                "targeting": "true",
                "userFacingDescription":"This is a test experiment for diagnostic purposes.",
            }),
            json!({
                "schemaVersion": "1.5.0",
                "slug": "ppop-mobile-test",
                // "arguments": {}, // DEPRECATED
                // "application": "org.mozilla.firefox_beta", // DEPRECATED
                "appName": "fenix",
                "appId": "org.mozilla.firefox_beta",
                "channel": "beta",
                "userFacingName": "[ppop] Mobile test",
                "userFacingDescription": "test",
                "isEnrollmentPaused": false,
                "bucketConfig": {
                    "randomizationUnit": "nimbus_id",
                    "namespace": "fenix-default-browser-4",
                    "start": 0,
                    "count": 10000,
                    "total": 10000
                },
                "probeSets": [],
                // "outcomes": [], analysis specific, no need to round-trip
                "branches": [
                    {
                        "slug": "default_browser_newtab_banner",
                        "ratio": 100,
                        "feature": {
                            "featureId": "fenix-default-browser",
                            "enabled": true,
                            "value": {}
                        }
                    },
                    {
                        "slug": "default_browser_settings_menu",
                        "ratio": 100,
                        "feature": {
                            "featureId": "fenix-default-browser",
                            "enabled": true,
                            "value": {}
                        }
                    },
                    {
                        "slug": "default_browser_toolbar_menu",
                        "ratio": 100,
                        "feature": {
                            "featureId": "fenix-default-browser",
                            "enabled": true,
                            "value": {}
                        }
                    },
                    {
                        "slug": "control",
                        "ratio": 1,
                        "feature": {
                            "featureId": "fenix-default-browser",
                            "enabled": false,
                            "value": {}
                        }
                    }
                ],
                "targeting": "true",
                "startDate": "2021-05-10T12:38:49.699091Z",
                "endDate": null,
                "proposedDuration": 28,
                "proposedEnrollment": 7,
                "referenceBranch": "control",
                "featureIds": [
                    "fenix-default-browser"
                ]
            }),
        ]
    }
    /// Each of this should uniquely reference a single experiment returned
    /// from get_db_v1_experiments_with_non_empty_features()
    fn get_db_v1_enrollments_with_non_empty_features() -> Vec<serde_json::Value> {
        vec![json!(
            {
                "slug": "secure-gold",
                "status":
                    {
                        "Enrolled":
                            {
                                "enrollment_id": "801ee64b-0b1b-44a7-be47-5f1b5c189083", // change when copy/pasting to make new
                                "reason": "Qualified",
                                "branch": "control",
                                "feature_id": "abc" // change on cloning
                            }
                        }
                    }
        )]
    }

    fn get_db_v1_experiments_with_missing_feature_fields() -> Vec<serde_json::Value> {
        vec![
            json!({
                "schemaVersion": "1.0.0",
                "slug": "branch-feature-empty-obj", // change when copy/pasting to make experiments
                "endDate": null,
                "branches":[
                    {
                        "slug": "control",
                        "ratio": 1,
                        "feature": {}
                    },
                    {
                        "slug": "treatment",
                        "ratio":1,
                        "feature": {}
                    }
                ],
                "channel": "nightly",
                "probeSets":[],
                "startDate":null,
                "appName": "fenix",
                "appId": "org.mozilla.fenix",
                "bucketConfig":{
                    // Setup to enroll everyone by default.
                    "count":10_000,
                    "start":0,
                    "total":10_000,
                    "namespace":"branch-feature-empty-obj", // change when copy/pasting to make experiments
                    "randomizationUnit":"nimbus_id"
                },
                "userFacingName":"Diagnostic test experiment",
                "referenceBranch":"control",
                "isEnrollmentPaused":false,
                "proposedEnrollment":7,
                "userFacingDescription":"This is a test experiment for diagnostic purposes.",
            }),
            json!({
                "schemaVersion": "1.0.0",
                "slug": "missing-branch-feature-clause", // change when copy/pasting to make experiments
                "endDate": null,
                "featureIds": ["aaa"], // change when copy/pasting to make experiments
                "branches":[
                    {
                        "slug": "control",
                        "ratio": 1,
                    },
                    {
                        "slug": "treatment",
                        "ratio":1,
                        "feature": {
                            "featureId": "aaa", // change when copy/pasting to make experiments
                            "enabled": true,
                            "value": {},
                        }
                    }
                ],
                "channel": "nightly",
                "probeSets":[],
                "startDate":null,
                "appName": "fenix",
                "appId": "org.mozilla.fenix",
                "bucketConfig":{
                    // Setup to enroll everyone by default.
                    "count":10_000,
                    "start":0,
                    "total":10_000,
                    "namespace":"empty-branch-feature-clause", // change when copy/pasting to make experiments
                    "randomizationUnit":"nimbus_id"
                },
                "userFacingName":"Diagnostic test experiment",
                "referenceBranch":"control",
                "isEnrollmentPaused":false,
                "proposedEnrollment":7,
                "userFacingDescription":"This is a test experiment for diagnostic purposes.",
            }),
            json!({
                "schemaVersion": "1.0.0",
                "slug": "branch-feature-feature-id-missing", // change when copy/pasting to make experiments
                "endDate": null,
                "featureIds": ["ccc"], // change when copy/pasting to make experiments
                "branches":[
                    {
                        "slug": "control",
                        "ratio": 1,
                        "feature": {
                            "featureId": "ccc", // change when copy/pasting to make experiments
                            "enabled": false,
                            "value": {}
                        }
                    },
                    {
                        "slug": "treatment",
                        "ratio":1,
                        "feature": {
                            "enabled": true,
                            "value": {}
                        }
                    }
                ],
                "channel": "nightly",
                "probeSets":[],
                "startDate":null,
                "appName": "fenix",
                "appId": "org.mozilla.fenix",
                "bucketConfig":{
                    // Setup to enroll everyone by default.
                    "count":10_000,
                    "start":0,
                    "total":10_000,
                    "namespace":"branch-feature-feature-id-missing", // change when copy/pasting to make experiments
                    "randomizationUnit":"nimbus_id"
                },
                "userFacingName":"Diagnostic test experiment",
                "referenceBranch":"control",
                "isEnrollmentPaused":false,
                "proposedEnrollment":7,
                "userFacingDescription":"This is a test experiment for diagnostic purposes.",
            }),
            json!({
                "schemaVersion": "1.0.0",
                "slug": "feature-ids-array-has-empty_string", // change when copy/pasting to make experiments
                "endDate": null,
                "featureIds": [""], // change when copy/pasting to make experiments
                "branches":[
                    {
                        "slug": "control",
                        "ratio": 1,
                        "feature": {
                            "featureId": "def", // change when copy/pasting to make experiments
                            "enabled": false,
                            "value": {},
                        }
                    },
                    {
                        "slug": "treatment",
                        "ratio":1,
                        "feature": {
                            "featureId": "def", // change when copy/pasting to make experiments
                            "enabled": true,
                            "value": {}
                        }
                    }
                ],
                "channel": "nightly",
                "probeSets":[],
                "startDate":null,
                "appName": "fenix",
                "appId": "org.mozilla.fenix",
                "bucketConfig":{
                    // Setup to enroll everyone by default.
                    "count":10_000,
                    "start":0,
                    "total":10_000,
                    "namespace":"feature-ids-array-has-empty-string", // change when copy/pasting to make experiments
                    "randomizationUnit":"nimbus_id"
                },
                "userFacingName":"Diagnostic test experiment",
                "referenceBranch":"control",
                "isEnrollmentPaused":false,
                "proposedEnrollment":7,
                "userFacingDescription":"This is a test experiment for diagnostic purposes.",
            }),
            json!({
                "schemaVersion": "1.0.0",
                "slug": "missing-feature-ids-in-branch",
                "endDate": null,
                "featureIds": ["abc"],
                "branches":[
                    {
                        "slug": "control",
                        "ratio": 1,
                        "feature": {
                            "enabled": true,
                            "value": {}
                        }
                    },
                    {
                        "slug": "treatment",
                        "ratio": 1,
                        "feature": {
                            "enabled": true,
                            "value": {}
                        }
                    }
                ],
                "probeSets":[],
                "startDate":null,
                "appName":"fenix",
                "appId":"org.mozilla.fenix",
                "channel":"nightly",
                "bucketConfig":{
                    // Setup to enroll everyone by default.
                    "count":10_000,
                    "start":0,
                    "total":10_000,
                    "namespace":"no-feature-ids-at-all",
                    "randomizationUnit":"nimbus_id"
                },
                "userFacingName":"Diagnostic test experiment",
                "referenceBranch":"control",
                "isEnrollmentPaused":false,
                "proposedEnrollment":7,
                "userFacingDescription":"This is a test experiment for diagnostic purposes.",
            }),
            json!({
                "schemaVersion": "1.0.0",
                "slug": "missing-featureids-array", // change when copy/pasting to make experiments
                "endDate": null,
                "branches":[
                    {
                        "slug": "control",
                        "ratio": 1,
                        "feature": {
                            "featureId": "about_welcome", // change when copy/pasting to make experiments
                            "enabled": false,
                            "value": {}
                        }
                    },
                    {
                        "slug": "treatment",
                        "ratio":1,
                        "feature": {
                            "featureId": "about_welcome", // change when copy/pasting to make experiments
                            "enabled": true,
                            "value": {}
                        }
                    }
                ],
                "channel": "nightly",
                "probeSets":[],
                "startDate":null,
                "appName": "fenix",
                "appId": "org.mozilla.fenix",
                "bucketConfig":{
                    // Setup to enroll everyone by default.
                    "count":10_000,
                    "start":0,
                    "total":10_000,
                    "namespace":"valid-feature-experiment", // change when copy/pasting to make experiments
                    "randomizationUnit":"nimbus_id"
                },
                "userFacingName":"Diagnostic test experiment",
                "referenceBranch":"control",
                "isEnrollmentPaused":false,
                "proposedEnrollment":7,
                "userFacingDescription":"This is a test experiment for diagnostic purposes.",
            }),
            json!({
                "schemaVersion": "1.0.0",
                "slug": "branch-feature-feature-id-empty", // change when copy/pasting to make experiments
                "endDate": null,
                "featureIds": [""], // change when copy/pasting to make experiments
                "branches":[
                    {
                        "slug": "control",
                        "ratio": 1,
                        "feature": {
                            "featureId": "", // change when copy/pasting to make experiments
                            "enabled": false,
                            "value": {},
                        }
                    },
                    {
                        "slug": "treatment",
                        "ratio":1,
                        "feature": {
                            "featureId": "", // change when copy/pasting to make experiments
                            "enabled": true,
                            "value": {},
                        }
                    }
                ],
                "channel": "nightly",
                "probeSets":[],
                "startDate":null,
                "appName": "fenix",
                "appId": "org.mozilla.fenix",
                "bucketConfig":{
                    // Setup to enroll everyone by default.
                    "count":10_000,
                    "start":0,
                    "total":10_000,
                    "namespace":"branch-feature-feature-id-empty", // change when copy/pasting to make experiments
                    "randomizationUnit":"nimbus_id"
                },
                "userFacingName":"Diagnostic test experiment",
                "referenceBranch":"control",
                "isEnrollmentPaused":false,
                "proposedEnrollment":7,
                "userFacingDescription":"This is a test experiment for diagnostic purposes.",
            }),
        ]
    }

    fn get_v1_enrollments_with_missing_feature_ids() -> Vec<serde_json::Value> {
        vec![
            json!({
                "slug": "feature-id-missing",
                "status":
                    {
                        "Enrolled":
                            {
                                "enrollment_id": "801ee64b-0b1b-47a7-be47-5f1b5c189084",
                                "reason": "Qualified",
                                "branch": "control",
                            }
                    }
            }),
            json!({
                "slug": "feature-id-empty",
                "status":
                    {
                        "Enrolled":
                            {
                                "enrollment_id": "801ee64b-0b1b-44a7-be47-5f1b5c189086",
                                "reason": "Qualified",
                                "branch": "control",
                                "feature_id": ""
                            }
                        }
            }),
        ]
    }

    /// Create a database with an old database version number, and
    /// populate it with the given experiments and enrollments.
    fn create_old_database(
        tmp_dir: &TempDir,
        old_version: u16,
        experiments_json: &[serde_json::Value],
        enrollments_json: &[serde_json::Value],
    ) -> Result<()> {
        let _ = env_logger::try_init();

        let rkv = Database::open_rkv(&tmp_dir)?;
        let meta_store = SingleStore::new(rkv.open_single("meta", StoreOptions::create())?);
        let experiment_store =
            SingleStore::new(rkv.open_single("experiments", StoreOptions::create())?);
        let enrollment_store =
            SingleStore::new(rkv.open_single("enrollments", StoreOptions::create())?);
        let mut writer = rkv.write()?;

        meta_store.put(&mut writer, "db_version", &old_version)?;

        // write out the experiments
        for experiment_json in experiments_json {
            // log::debug!("experiment_json = {:?}", experiment_json);
            experiment_store.put(
                &mut writer,
                experiment_json["slug"].as_str().unwrap(),
                experiment_json,
            )?;
        }

        // write out the enrollments
        for enrollment_json in enrollments_json {
            // log::debug!("enrollment_json = {:?}", enrollment_json);
            enrollment_store.put(
                &mut writer,
                enrollment_json["slug"].as_str().unwrap(),
                enrollment_json,
            )?;
        }

        writer.commit()?;
        log::debug!("create_old_database committed");

        Ok(())
    }

    #[test]
    /// Migrating db v1 to db v2 involves finding enrollments that
    /// don't contain all the feature stuff they should and discarding.
    /// It will also discard other experiments/enrollments with required
    /// headers that are missing.
    fn test_migrate_db_v1_to_db_v2_enrollment_discarding() -> Result<()> {
        let _ = env_logger::try_init();
        let tmp_dir = TempDir::new("migrate_db_v1_to_db_v2")?;

        // write invalid enrollments
        let db_v1_enrollments_with_missing_feature_ids =
            &get_v1_enrollments_with_missing_feature_ids();

        create_old_database(&tmp_dir, 1, &[], db_v1_enrollments_with_missing_feature_ids)?;
        let db = Database::new(&tmp_dir)?;

        // The enrollments with invalid feature_ids should have been discarded
        // during migration; leaving us with none.
        let enrollments = db
            .collect_all::<ExperimentEnrollment>(StoreId::Enrollments)
            .unwrap();
        //log::debug!("enrollments = {:?}", enrollments);

        assert_eq!(enrollments.len(), 0);

        Ok(())
    }

    /// Migrating v1 to v2 involves finding experiments that
    /// don't contain all the feature stuff they should and discarding.
    #[test]
    fn test_migrate_db_v1_to_db_v2_experiment_discarding() -> Result<()> {
        let _ = env_logger::try_init();
        let tmp_dir = TempDir::new("migrate_db_v1_to_db_v2_enrollment_discarding")?;

        // write a bunch of invalid experiments
        let db_v1_experiments_with_missing_feature_fields =
            &get_db_v1_experiments_with_missing_feature_fields();

        create_old_database(
            &tmp_dir,
            1,
            db_v1_experiments_with_missing_feature_fields,
            &[],
        )?;

        let db = Database::new(&tmp_dir)?;

        // All of the experiments with invalid FeatureConfig related stuff
        // should have been discarded during migration; leaving us with none.
        let experiments = db.collect_all::<Experiment>(StoreId::Experiments).unwrap();
        log::debug!("experiments = {:?}", experiments);

        assert_eq!(experiments.len(), 0);

        Ok(())
    }

    #[test]
    fn test_migrate_db_v1_to_db_v2_round_tripping() -> Result<()> {
        let _ = env_logger::try_init();
        let tmp_dir = TempDir::new("migrate_round_tripping")?;

        // write valid experiments & enrollments
        let db_v1_experiments_with_non_empty_features =
            &db_v1_experiments_with_non_empty_features();
        // ... and enrollments
        let db_v1_enrollments_with_non_empty_features =
            &get_db_v1_enrollments_with_non_empty_features();

        create_old_database(
            &tmp_dir,
            1,
            db_v1_experiments_with_non_empty_features,
            db_v1_enrollments_with_non_empty_features,
        )?;

        // force an upgrade & read in the upgraded database
        let db = Database::new(&tmp_dir).unwrap();

        let db_experiments = db.collect_all::<Experiment>(StoreId::Experiments)?;
        // XXX hoist into build_map function (we build maps because they
        // compensate for the fact that iters don't return things in a
        // deterministic order).
        let db_experiment_map: HashMap<String, serde_json::Value> = db_experiments
            .into_iter()
            .map(|e| {
                let e_json = serde_json::to_value::<Experiment>(e.clone()).unwrap();
                let e_slug = e.slug;
                (e_slug, e_json)
            })
            .collect();

        // XXX hoist into build_map function
        let orig_experiment_map: HashMap<String, serde_json::Value> =
            db_v1_experiments_with_non_empty_features
                .iter()
                .map(|e_ref| {
                    let e = e_ref.clone();
                    let e_slug = e.get("slug").unwrap().as_str().unwrap().to_string();
                    (e_slug, e)
                })
                .collect();

        // The original json should be the same as data that's gone through
        // migration, put into the rust structs again, and pulled back out.
        assert_eq!(&orig_experiment_map, &db_experiment_map);
        // log::debug!("db_experiments = {:?}", &db_experiment_map);

        let enrollments = db.collect_all::<ExperimentEnrollment>(StoreId::Enrollments)?;

        // XXX hoist into build_map function
        let db_enrollments: HashMap<String, serde_json::Value> = enrollments
            .into_iter()
            .map(|e| {
                let e_json = serde_json::to_value::<ExperimentEnrollment>(e).unwrap();
                let mut e_slug: String = String::new();
                e_slug.push_str(e_json.get("slug").unwrap().as_str().unwrap());
                (e_slug, e_json)
            })
            .collect();

        // XXX hoist into build_map function
        let orig_enrollments: HashMap<String, serde_json::Value> =
            db_v1_enrollments_with_non_empty_features
                .iter()
                .map(|e_ref| {
                    let e = e_ref.clone();
                    let mut e_slug: String = String::new();
                    e_slug.push_str(e.get("slug").unwrap().as_str().unwrap());
                    (e_slug, e)
                })
                .collect();

        // The original json should be the same as data that's gone through
        // migration, put into the rust structs again, and pulled back out.
        assert_eq!(&orig_enrollments, &db_enrollments);
        // log::debug!("db_enrollments = {:?}", db_enrollments);

        Ok(())
    }

    /// Migrating db_v1 to db_v2 involves finding enrollments and experiments that
    /// don't contain all the feature_id stuff they should and discarding.
    #[test]
    fn test_migrate_db_v1_with_valid_and_invalid_records_to_db_v2() -> Result<()> {
        let experiment_with_feature = json!({
            "schemaVersion": "1.0.0",
            "slug": "secure-gold",
            "endDate": null,
            "featureIds": ["about_welcome"],
            "branches":[
                {
                    "slug": "control",
                    "ratio": 1,
                    "feature": {
                        "featureId": "about_welcome",
                        "enabled": false
                    }
                },
                {
                    "slug": "treatment",
                    "ratio":1,
                    "feature": {
                        "featureId": "about_welcome",
                        "enabled": true
                    }
                }
            ],
            "channel": "nightly",
            "probeSets":[],
            "startDate":null,
            "appName": "fenix",
            "appId": "org.mozilla.fenix",
            "bucketConfig":{
                // Setup to enroll everyone by default.
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
        });

        let enrollment_with_feature = json!(
            {
                "slug": "secure-gold",
                "status":
                    {
                        "Enrolled":
                            {
                                "enrollment_id": "801ee64b-0b1b-44a7-be47-5f1b5c189084",// XXXX should be client id?
                                "reason": "Qualified",
                                "branch": "control",
                                "feature_id": "about_welcome"
                            }
                        }
                    }
        );

        let experiment_without_feature = json!(
        {
            "schemaVersion": "1.0.0",
            "slug": "no-features",
            "endDate": null,
            "branches":[
                {
                    "slug": "control",
                    "ratio": 1,
                },
                {
                    "slug": "treatment",
                    "ratio": 1,
                }
            ],
            "probeSets":[],
            "startDate":null,
            "appName":"fenix",
            "appId":"org.mozilla.fenix",
            "channel":"nightly",
            "bucketConfig":{
                // Setup to enroll everyone by default.
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
            "id":"no-features",
            "last_modified":1_602_197_324_372i64
        });

        let enrollment_without_feature = json!(
            {
                "slug": "no-features",
                "status":
                    {
                        "Enrolled":
                            {
                                "enrollment_id": "801ee64b-0b1b-47a7-be47-5f1b5c189084",
                                "reason": "Qualified",
                                "branch": "control",
                            }
                    }
            }
        );

        use tempdir::TempDir;

        let tmp_dir = TempDir::new("test_drop_experiments_wo_feature_id")?;
        let _ = env_logger::try_init();

        create_old_database(
            &tmp_dir,
            1,
            &[experiment_with_feature, experiment_without_feature],
            &[enrollment_with_feature, enrollment_without_feature],
        )?;

        let db = Database::new(&tmp_dir)?;

        let experiments = db.collect_all::<Experiment>(StoreId::Experiments).unwrap();
        log::debug!("experiments = {:?}", experiments);

        // The experiment without features should have been discarded, leaving
        // us with only one.
        assert_eq!(experiments.len(), 1);

        let enrollments = db
            .collect_all::<ExperimentEnrollment>(StoreId::Enrollments)
            .unwrap();
        log::debug!("enrollments = {:?}", enrollments);

        // The enrollment without features should have been discarded, leaving
        // us with only one.
        assert_eq!(enrollments.len(), 1);

        Ok(())
    }

    // XXX Ideally, we would also write test to ensure that anytime one of
    // (enrollment, experiment) an invalid featureAPI issue, both the
    // experiment and the enrollment are removed from their respective stores
    // so we don't have any weird orphans
}

// TODO: Add unit tests
// Possibly by using a trait for persistence and mocking it to test the persistence.
