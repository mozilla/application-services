/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use crate::mars::Environment;

pub(crate) struct AdsClientConfig {
    pub cache_config: Option<AdsCacheConfig>,
    pub environment: Environment,
    #[cfg(feature = "stateful")]
    pub store_config: Option<AdsStoreConfig>,
    /// The browser's user agent, sent as `X-User-Agent` on OHTTP requests
    /// when the preflight returns no normalized user agent.
    pub user_agent: Option<String>,
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
