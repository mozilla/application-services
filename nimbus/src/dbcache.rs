// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::enrollment::get_enrollments;
use crate::error::{Error, Result};
use crate::persistence::Database;
use std::collections::HashMap;
use std::sync::RwLock;

// This module manages an in-memory cache of the database, so that some
// functions exposed by nimbus can return results without blocking on any
// IO. Consumers are expected to call our public `update()` function whenever
// the database might have changed.

// This struct is the cached data. This is never mutated, but instead
// recreated every time the cache is updated.
struct CachedData {
    pub experiment_branches: HashMap<String, String>,
}

// This is the public cache API. Each NimbusClient can create one of these and
// it lives as long as the client - it encapsulates the synchronization needed
// to allow the cache to work correctly.
#[derive(Default)]
pub struct DatabaseCache {
    data: RwLock<Option<CachedData>>,
}

impl DatabaseCache {
    // Call this function whenever it's possible that anything cached by this
    // struct (eg, our enrollments) might have changed. It is passed a
    // &Database, which implies some mutex guarding that Database is held.
    pub fn update(&self, db: &Database) -> Result<()> {
        let experiments = get_enrollments(&db)?;
        // Build the new hashmap.
        let mut eb = HashMap::with_capacity(experiments.len());
        for e in experiments {
            eb.insert(e.slug, e.branch_slug);
        }
        let data = CachedData {
            experiment_branches: eb,
        };
        // then swap it in.
        self.data.write().unwrap().replace(data);
        Ok(())
    }

    // Abstracts safely referencing our cached data.
    //
    // WARNING: because this manages locking, the callers of this need to be
    // careful regarding deadlocks - if the callback takes other own locks then
    // there's a risk of locks being taken in an inconsistent order. However,
    // there's nothing this code specifically can do about that.
    fn get_data<T, F>(&self, func: F) -> Result<T>
    where
        F: FnOnce(&CachedData) -> T,
    {
        match *self.data.read().unwrap() {
            None => {
                log::warn!(
                    "DatabaseCache attempting to read data before initialization is completed"
                );
                Err(Error::DatabaseNotReady)
            }
            Some(ref data) => Ok(func(data)),
        }
    }

    pub fn get_experiment_branch(&self, slug: &str) -> Result<Option<String>> {
        self.get_data(|data| data.experiment_branches.get(slug).cloned())
    }
}
