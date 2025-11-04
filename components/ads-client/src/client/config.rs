/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

#[derive(Default, uniffi::Record)]
pub struct MozAdsClientConfig {
    pub environment: Environment,
    pub cache_config: Option<MozAdsCacheConfig>,
}

#[derive(Clone, Copy, Debug, Default, uniffi::Enum, Eq, PartialEq)]
pub enum Environment {
    #[default]
    Prod,
    #[cfg(feature = "dev")]
    Staging,
}

#[derive(uniffi::Record)]
pub struct MozAdsCacheConfig {
    pub db_path: String,
    pub default_cache_ttl_seconds: Option<u64>,
    pub max_size_mib: Option<u64>,
}
