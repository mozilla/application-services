/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use crate::mars::Environment;
use crate::telemetry::Telemetry;

pub struct AdsClientConfig<T>
where
    T: Telemetry,
{
    pub cache_config: Option<AdsCacheConfig>,
    pub context_id_provider: Option<Box<dyn super::ContextIdProvider>>,
    pub environment: Environment,
    pub telemetry: T,
}

#[derive(Clone, Debug)]
pub struct AdsCacheConfig {
    pub db_path: String,
    pub default_cache_ttl_seconds: Option<u64>,
    pub max_size_mib: Option<u64>,
}
