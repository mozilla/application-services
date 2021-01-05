/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Our storage abstraction, currently backed by Rkv.

use crate::error::{Error, Result};
// This uses the lmdb backend for rkv, which is unstable.
// We use it for now since glean didn't seem to have trouble with it (although
// it must be noted that the rkv documentation explicitly says "To use rkv in
// production/release environments at Mozilla, you may do so with the "SafeMode"
// backend", so we really should get more guidance here.)
use core::iter::Iterator;
use rkv::StoreOptions;
use std::fs;
use std::path::Path;

const DB_KEY_DB_VERSION: &str = "db_version";
const DB_VERSION: u16 = 1; // Increment and implement a DB migration in `maybe_upgrade` when necessary.

// Inspired by Glean - use a feature to choose between the backends.
// Select the LMDB-powered storage backend when the feature is not activated.
#[cfg(not(feature = "rkv-safe-mode"))]
mod backend {
    use std::path::Path;
    //use rkv::Readable;
    use rkv::backend::{Lmdb, LmdbDatabase, LmdbEnvironment, LmdbRwTransaction};

    pub type Rkv = rkv::Rkv<LmdbEnvironment>;
    pub type RkvSingleStore = rkv::SingleStore<LmdbDatabase>;
    pub type Writer<'t> = rkv::Writer<LmdbRwTransaction<'t>>;

    pub fn rkv_new(path: &Path) -> Result<Rkv, rkv::StoreError> {
        Rkv::new::<Lmdb>(path)
    }
}

// Select the "safe mode" storage backend when the feature is activated.
#[cfg(feature = "rkv-safe-mode")]
mod backend {
    use rkv::backend::{SafeMode, SafeModeDatabase, SafeModeEnvironment, SafeModeRwTransaction};
    use std::path::Path;

    pub type Rkv = rkv::Rkv<SafeModeEnvironment>;
    pub type RkvSingleStore = rkv::SingleStore<SafeModeDatabase>;
    pub type Writer<'t> = rkv::Writer<SafeModeRwTransaction<'t>>;

    pub fn rkv_new(path: &Path) -> Result<Rkv, rkv::StoreError> {
        Rkv::new::<SafeMode>(path)
    }
}

pub use backend::Writer;
use backend::*;

//#[derive(Copy, Clone)]
pub enum StoreId {
    Experiments,
    Enrollments,
    Meta,
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
    pub fn get<T: serde::Serialize + for<'de> serde::Deserialize<'de>>(
        &self,
        writer: &Writer,
        key: &str,
    ) -> Result<Option<T>> {
        let persisted_data = self.store.get(writer, key)?;
        match persisted_data {
            Some(data) => {
                if let rkv::Value::Json(data) = data {
                    Ok(Some(serde_json::from_str::<T>(data)?))
                } else {
                    Err(Error::InvalidPersistedData)
                }
            }
            None => Ok(None),
        }
    }

    pub fn collect_all<T: serde::Serialize + for<'de> serde::Deserialize<'de>>(
        &self,
        writer: &Writer,
    ) -> Result<Vec<T>> {
        let mut result = Vec::new();
        let mut iter = self.store.iter_start(writer)?;
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
        let mut writer = self.rkv.write()?;
        let db_version = self.meta_store.get::<u16>(&writer, DB_KEY_DB_VERSION)?;
        match db_version {
            Some(DB_VERSION) => return Ok(()),
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
        // accidentally unenrol.
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
        let rkv = rkv_new(&path)?;
        log::debug!("Database initialized");
        Ok(rkv)
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
    ///
    /// # Arguments
    /// - `key`: A key for the data stored in the underlying database
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
                    Err(Error::InvalidPersistedData)
                }
            }
            None => Ok(None),
        }
    }

    // Iters are a bit tricky - would be nice to make them generic, but this will
    // do for our use-case.
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
}

// TODO: Add unit tests
// Possibly by using a trait for persistence and mocking it to test the persistence.
