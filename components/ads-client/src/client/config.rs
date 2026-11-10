/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

#[cfg(feature = "stateful")]
use std::time::Duration;

use crate::mars::Environment;
use crate::non_empty_string::NonEmptyString;

pub(crate) struct AdsClientConfig {
    pub cache_config: Option<AdsCacheConfig>,
    pub environment: Environment,
    #[cfg(feature = "stateful")]
    pub store_config: Option<AdsStoreConfig>,
    pub user_agent: Option<NonEmptyString>,
    // TODO: Add note: I like separating these like this, but we could condense these onto AdsClientConfig.
    #[cfg(feature = "stateful")]
    pub worker_config: Option<WorkerConfig>,
}

#[derive(Clone, Debug)]
pub(crate) struct AdsCacheConfig {
    pub db_path: String,
    pub default_cache_ttl_seconds: Option<u64>,
    pub max_size_mib: Option<u64>,
}

#[cfg(feature = "stateful")]
#[derive(Clone, Debug)]
pub(crate) struct AdsStoreConfig {
    pub db_path: String,
}

#[cfg(feature = "stateful")]
#[derive(Clone, Debug, Default)]
pub struct WorkerConfig {
    pub worker_batch: Option<Duration>,
}
