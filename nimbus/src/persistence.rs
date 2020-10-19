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
use rkv::{Rkv, SingleStore, StoreOptions};
use std::fs;
use std::path::Path;

pub enum StoreId {
    Experiments,
    Enrollments,
    Meta,
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
    #[allow(unused)]
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
            meta_store,
            experiment_store,
            enrollment_store,
        })
    }

    fn get_store(&self, store_id: StoreId) -> &SingleStore {
        match store_id {
            StoreId::Meta => &self.meta_store,
            StoreId::Experiments => &self.experiment_store,
            StoreId::Enrollments => &self.enrollment_store,
        }
    }

    #[allow(unused)]
    fn open_rkv<P: AsRef<Path>>(path: P) -> Result<Rkv> {
        let path = std::path::Path::new(path.as_ref()).join("db");
        log::debug!("Database path: {:?}", path.display());
        fs::create_dir_all(&path)?;
        let rkv = Rkv::new(&path)?;
        log::debug!("Database initialized");
        Ok(rkv)
    }

    /// Function used to retrieve persisted data
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
        let persisted_data = self.get_store(store_id).get(&reader, key)?;
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

    /// Function used to persist data
    /// It allows the persistence of any serializable and deserializable data
    /// Currently only supports JSON data
    ///
    /// # Arguments
    /// - `key`: The key for the persisted data, this is what will be used in the `get` function to retreive the data
    /// - `persisted_data`: The data to be persisted
    pub fn put<T: serde::Serialize + for<'de> serde::Deserialize<'de>>(
        &self,
        store_id: StoreId,
        key: &str,
        persisted_data: &T,
    ) -> Result<()> {
        let mut writer = self.rkv.write()?;
        let persisted_json = serde_json::to_string(persisted_data)?;
        self.get_store(store_id)
            .put(&mut writer, key, &rkv::Value::Json(&persisted_json))?;
        writer.commit()?;
        Ok(())
    }

    /// Delete the specified value
    pub fn delete(&self, store_id: StoreId, key: &str) -> Result<()> {
        let mut writer = self.rkv.write()?;
        self.get_store(store_id).delete(&mut writer, key)?;
        writer.commit()?;
        Ok(())
    }

    /// Clear all values.
    pub fn clear(&self, store_id: StoreId) -> Result<()> {
        let mut writer = self.rkv.write()?;
        self.get_store(store_id).clear(&mut writer)?;
        writer.commit()?;
        Ok(())
    }

    // Iters are a bit tricky - would be nice to make them generic, but this will
    // do for our use-case.
    pub fn collect_all<T: serde::Serialize + for<'de> serde::Deserialize<'de>>(
        &self,
        store_id: StoreId,
    ) -> Result<Vec<T>> {
        let mut result = Vec::new();
        let reader = self.rkv.read()?;
        let mut iter = self.get_store(store_id).iter_start(&reader)?;
        while let Some(Ok((_, data))) = iter.next() {
            if let Some(rkv::Value::Json(data)) = data {
                result.push(serde_json::from_str::<T>(&data)?);
            }
        }
        Ok(result)
    }

    pub fn has_any(&self, store_id: StoreId) -> Result<bool> {
        let reader = self.rkv.read()?;
        let mut iter = self.get_store(store_id).iter_start(&reader)?;
        Ok(iter.next().is_some())
    }
}

// TODO: Add unit tests
// Possibly by using a trait for persistence and mocking it to test the persistence.
