// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::enrollment::{get_enrollments, map_features_by_feature_id, EnrolledFeatureConfig};
use crate::error::{NimbusError, Result};
use crate::persistence::{Database, StoreId, Writer};
use crate::{enrollment::ExperimentEnrollment, Experiment};
use std::collections::HashMap;
use std::sync::RwLock;

// This module manages an in-memory cache of the database, so that some
// functions exposed by nimbus can return results without blocking on any
// IO. Consumers are expected to call our public `update()` function whenever
// the database might have changed.

// This struct is the cached data. This is never mutated, but instead
// recreated every time the cache is updated.
struct CachedData {
    pub branches_by_experiment: HashMap<String, String>,
    pub features_by_feature_id: HashMap<String, EnrolledFeatureConfig>,
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
    // struct (eg, our enrollments) might have changed.
    //
    // This function must be passed a `&Database` and a `Writer`, which it
    // will commit before updating the in-memory cache. This is a slightly weird
    // API but it helps encorce two important properties:
    //
    //  * By requiring a `Writer`, we ensure mutual exclusion of other db writers
    //    and thus prevent the possibility of caching stale data.
    //  * By taking ownership of the `Writer`, we ensure that the calling code
    //    updates the cache after all of its writes have been performed.
    pub fn commit_and_update(&self, db: &Database, writer: Writer) -> Result<()> {
        // By passing in the active `writer` we read the state of enrollments
        // as written by the calling code, before it's committed to the db.
        let enrollments = get_enrollments(db, &writer)?;

        // Build the new hashmaps.  Note that this is somewhat temporary, is
        // likely to change when the full FeatureConfig stuff is implemented.
        // Further, note that, for the moment, we only (currently) support
        // one feature_id per experiment, meaning that we ignore everything
        // except the first feature_id in the array.  Some of the multi-feature
        // code may want to live in the EnrollmentEvolver.
        let mut branches_by_experiment = HashMap::with_capacity(enrollments.len());

        for e in enrollments {
            branches_by_experiment.insert(e.slug, e.branch_slug.clone());
        }

        let enrollments: Vec<ExperimentEnrollment> =
            db.get_store(StoreId::Enrollments).collect_all(&writer)?;
        let experiments: Vec<Experiment> =
            db.get_store(StoreId::Experiments).collect_all(&writer)?;

        let features_by_feature_id = map_features_by_feature_id(&enrollments, &experiments);

        // This is where testing tools would override i.e. replace experimental feature configurations.
        // i.e. testing tools would cause custom feature configs to be stored in a Store.
        // Here, we get those overrides out of the store, and merge it with this map.

        // This is where rollouts (promoted experiments on a given feature) will be merged in to the feature variables.

        let data = CachedData {
            branches_by_experiment,
            features_by_feature_id,
        };

        // Try to commit the change to disk and update the cache as close
        // together in time as possible. This leaves a small window where another
        // thread could read new data from disk but see old data in the cache,
        // but that seems benign in practice given the way we use the cache.
        // The alternative would be to lock the cache while we commit to disk,
        // and we don't want to risk blocking the main thread.
        writer.commit()?;
        let mut cached = self.data.write().unwrap();
        cached.replace(data);
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
                Err(NimbusError::DatabaseNotReady)
            }
            Some(ref data) => Ok(func(data)),
        }
    }

    pub fn get_experiment_branch(&self, id: &str) -> Result<Option<String>> {
        self.get_data(|data| match data.features_by_feature_id.get(id) {
            None => data.branches_by_experiment.get(id).cloned(),
            Some(feature) => Some(feature.branch.clone()),
        })
    }

    pub fn get_feature_config_variables(&self, feature_id: &str) -> Result<Option<String>> {
        self.get_data(|data| {
            if let Some(enrolled_feature) = data.features_by_feature_id.get(feature_id) {
                let string = serde_json::to_string(&enrolled_feature.feature.value).unwrap();
                Some(string)
            } else {
                None
            }
        })
    }
}
