// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::enrollment::get_enrollments;
use crate::error::{Error, Result};
use crate::persistence::Database;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Mutex;

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
// it lives as long as the client - it encapsulates the cells and locking
// needed to allow the cache to work correctly.
// WARNING: because this manages locking, the callers of this need to be
// careful regarding deadlocks - if the callers take their own locks (which
// they will!), there's always a risk of locks being taken in an inconsistent
// order. However, there's nothing this code specifically can do about that.
#[derive(Default)]
pub struct DatabaseCache {
    // Notes about the types here:
    // * We use a `Mutex` because a `RwLock` doesn't make the inner object
    //   `Sync`, which we require. An alternative would be
    //   `RwLock<AtomicRefCell<...>>` but we don't have an existing dependency
    //   on AtomicRefCell and it seems wierd to add one just to micro-optimize
    //   away from `Mutex` when our uses-cases, in practice, fine with a mutex.
    //   However, it is worth noting that mozilla-central does depend on
    //   `AtomicRefCell` so it wouldn't be *that* difficult to argue for the
    //   new dependency, it's just that no one has yet :)
    // * We use a `RefCell` even though we don't mutate it in place,
    //   because `Cell::get()` requires the data to be copied, which a HashMap
    //   doesn't offer.
    data: Mutex<RefCell<Option<CachedData>>>,
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
        let cell = self.data.lock().unwrap();
        cell.replace(Some(data));
        Ok(())
    }

    // Abstracts safely referencing our cached data.
    fn get_data<T, F>(&self, func: F) -> Result<T>
    where
        F: FnOnce(&CachedData) -> T,
    {
        let guard = self.data.lock().unwrap();
        let r = guard.borrow();
        let data = r.as_ref();
        match data {
            None => {
                log::warn!(
                    "DatabaseCache attempting to read data before initialization is completed"
                );
                Err(Error::DatabaseNotReady)
            }
            Some(data) => Ok(func(data)),
        }
    }

    pub fn get_experiment_branch(&self, slug: &str) -> Result<Option<String>> {
        self.get_data(|data| data.experiment_branches.get(slug).cloned())
    }
}
