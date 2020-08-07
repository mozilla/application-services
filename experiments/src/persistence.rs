/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This is where the persistence logic might go.
//! An idea for what to use here might be [RKV](https://github.com/mozilla/rkv)
//! And that's what's used on this prototype,
//! Either ways, the solution implemented should work regardless of the platform
//! on the other side of the FFI. This means that this module might require the FFI to allow consumers
//! To pass in a path to a database, or somewhere in the file system that the state will be persisted

use crate::error::{Error, Result};
// This uses the lmdb backend for rkv, which is unstable.
// We use it for now since glean didn't seem to have trouble with it
use rkv::{Rkv, SingleStore, StoreOptions};
use std::fs;
use std::path::Path;

/// Database used to access persisted data
/// This an abstraction around an Rkv database
/// An instance on this database is created each time the component is loaded
/// if there is persisted data, the `get` functions should retrieve it
pub struct Database {
    rkv: Rkv,
    experiment_store: SingleStore,
}

impl Database {
    #[allow(unused)]
    /// Main constructor for a database
    /// Initiates the Rkv database to be used to retreive persisted data
    /// # Arguments
    /// - `path`: A path to the persisted data, this is provided by the consuming application
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let rkv = Self::open_rkv(path)?;
        let experiment_store = rkv.open_single("experiments", StoreOptions::create())?;
        Ok(Self {
            rkv,
            experiment_store,
        })
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

    #[allow(unused)]
    /// Function used to retrieve persisted data
    /// It allows retrieval of any serializable and deserializable data
    /// Currently only supports JSON data
    ///
    /// # Arguments
    /// - `key`: A key for the data stored in the underlying database
    pub fn get<T: serde::Serialize + for<'de> serde::Deserialize<'de>>(
        &self,
        key: &str,
    ) -> Result<Option<T>> {
        let reader = self.rkv.read()?;
        let persisted_data = self.experiment_store.get(&reader, key)?;
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

    #[allow(unused)]
    /// Function used to persist data
    /// It allows the persistence of any serializable and deserializable data
    /// Currently only supports JSON data
    ///
    /// # Arguments
    /// - `key`: The key for the persisted data, this is what will be used in the `get` function to retreive the data
    /// - `persisted_data`: The data to be persisted
    pub fn put<T: serde::Serialize + for<'de> serde::Deserialize<'de>>(
        &self,
        key: &str,
        persisted_data: &T,
    ) -> Result<()> {
        let mut writer = self.rkv.write()?;
        let persisted_json = serde_json::to_string(persisted_data)?;
        self.experiment_store
            .put(&mut writer, key, &rkv::Value::Json(&persisted_json))?;
        writer.commit()?;
        Ok(())
    }
}

// TODO: Add unit tests
// Possibly by using a trait for persistence and mocking it to test the persistence.
