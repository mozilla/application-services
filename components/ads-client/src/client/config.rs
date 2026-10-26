/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

#[cfg(feature = "stateful")]
use std::time::Duration;

use crate::mars::Environment;

pub struct AdsClientConfig {
    pub cache_config: Option<AdsCacheConfig>,
    pub environment: Environment,
    #[cfg(feature = "stateful")]
    pub store_config: Option<AdsStoreConfig>,
    #[cfg(feature = "stateful")]
    pub worker_config: Option<AdsWorkerConfig>,
}

#[derive(Clone, Debug)]
pub struct AdsCacheConfig {
    pub db_path: String,
    pub default_cache_ttl_seconds: Option<u64>,
    pub max_size_mib: Option<u64>,
}

#[cfg(feature = "stateful")]
#[derive(Clone, Debug)]
pub struct AdsStoreConfig {
    pub db_path: String,
}

#[cfg(feature = "stateful")]
#[derive(Clone, Debug)]
pub struct AdsWorkerConfig {
    pub batch_interval: Option<Duration>,
}
