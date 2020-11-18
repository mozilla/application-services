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
}

impl Database {
    /// Main constructor for a database
    /// Initiates the Rkv database to be used to retreive persisted data
    /// # Arguments
    /// - `path`: A path to the persisted data, this is provided by the consuming application
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let rkv = Self::open_rkv(path)?;
        let meta_store = rkv.open_single("meta", StoreOptions::create())?;
        // TODO: we probably want a simple "version" key that we insist matches,
        // and if it doesn't we discard the DB and start again?
        let experiment_store = rkv.open_single("experiments", StoreOptions::create())?;
        let enrollment_store = rkv.open_single("enrollments", StoreOptions::create())?;
        Ok(Self {
            rkv,
            meta_store: SingleStore::new(meta_store),
            experiment_store: SingleStore::new(experiment_store),
            enrollment_store: SingleStore::new(enrollment_store),
        })
    }

    /// Gets a Store object, which used with the writer returned by
    /// `self.write()` to update the database in a transaction.
    pub fn get_store(&self, store_id: StoreId) -> &SingleStore {
        match store_id {
            StoreId::Meta => &self.meta_store,
            StoreId::Experiments => &self.experiment_store,
            StoreId::Enrollments => &self.enrollment_store,
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

// TODO: Add unit tests
// Possibly by using a trait for persistence and mocking it to test the persistence.
