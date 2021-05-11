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
use core::iter::Iterator;
use crate::enrollment::{EnrollmentStatus,ExperimentEnrollment};
use crate::Experiment;
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
            },
            Some(1) => {
                log::debug!("Upgrading from version 1 to version 2");
                // XXX how do we handle errors?
                // XXX Do we need to do anything extra for mutex & or
                // transaction?

                // iterate enrollments, with collect_all.
                // XXX later shift it to collect_all_json
                let reader = self.read()?;
                let enrollments: Vec<ExperimentEnrollment> =
                    self.enrollment_store.collect_all(&reader)?;
                let experiments: Vec<Experiment> =
                    self.experiment_store.collect_all(&reader)?;
                // XXX do we need to commit here?

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

                // find slugs and split apart those missing
                // feature_ids Vec
                //
                // XXX and later feature_id on branches, and
                // feature fields)

                let slugs_without_experiment_feature_ids: HashSet<String> = experiments
                    .iter()
                    .filter_map(
                            |e| {
                        if e.feature_ids.is_empty() {
                        log::warn!("Experiment for {:?} missing feature_ids; experiment & enrollment will be discarded", &e.slug);
                            Some(e.slug.to_owned())
                        } else {
                            None
                        }
                    })
                    .collect();

                let slugs_to_discard: HashSet<_> = slugs_without_enrollment_feature_ids
                    .union(&slugs_without_experiment_feature_ids).collect();

                // filter out experiments to be dropped
                let updated_experiments: Vec<Experiment> = experiments
                    .into_iter()
                    .filter(|e| !slugs_to_discard.contains(&e.slug))
                    .collect();

                // filter out enrollments to be dropped
                let updated_enrollments: Vec<ExperimentEnrollment> = enrollments
                    .into_iter()
                    .filter(|e| !slugs_to_discard.contains(&e.slug))
                    .collect();
                log::debug!("updated enrollments = {:?}", updated_enrollments);

                // rewrite stores
                for experiment in updated_experiments {
                    self.experiment_store.put(
                        &mut writer,&experiment.slug, &experiment)?;
                }
                for enrollment in updated_enrollments {
                    self.enrollment_store.put(
                        &mut writer,&enrollment.slug,&enrollment)?;
                }
            },
            None => {
                // The "first" version of the database (= no version number) had un-migratable data
                // for experiments and enrollments, start anew.
                // XXX: We can most likely remove this behaviour once enough time has passed,
                // since nimbus wasn't really shipped to production at the time anyway.
                self.experiment_store.clear(&mut writer)?;
                self.enrollment_store.clear(&mut writer)?;
            }
            _ => {
                log::error!("Unknown database version. Wiping everything.");
                self.meta_store.clear(&mut writer)?;
                self.experiment_store.clear(&mut writer)?;
                self.enrollment_store.clear(&mut writer)?;
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

    fn open_rkv<P: AsRef<Path>>(path: P) -> Result<Rkv> {
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
mod tests {
    use tempdir::TempDir;

    use super::*;

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
}

// TODO: Add unit tests
// Possibly by using a trait for persistence and mocking it to test the persistence.
