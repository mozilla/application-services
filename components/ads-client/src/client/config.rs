/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use once_cell::sync::Lazy;
use url::Url;

use crate::telemetry::Telemetry;

static MARS_API_ENDPOINT_PROD: Lazy<Url> =
    Lazy::new(|| Url::parse("https://ads.mozilla.org/v1/").expect("hardcoded URL must be valid"));

static MARS_API_ENDPOINT_STAGING: Lazy<Url> =
    Lazy::new(|| Url::parse("https://ads.allizom.org/v1/").expect("hardcoded URL must be valid"));

pub struct AdsClientConfig<T>
where
    T: Telemetry,
{
    pub environment: Environment,
    pub cache_config: Option<AdsCacheConfig>,
    pub telemetry: T,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Environment {
    #[default]
    Prod,
    Staging,
    #[cfg(test)]
    Test,
}

impl Environment {
    fn base_url(self) -> Url {
        match self {
            Environment::Prod => MARS_API_ENDPOINT_PROD.clone(),
            Environment::Staging => MARS_API_ENDPOINT_STAGING.clone(),
            #[cfg(test)]
            Environment::Test => Url::parse(&mockito::server_url()).unwrap(),
        }
    }

    pub fn into_url(self, path: &str) -> Url {
        let mut base = self.base_url();
        // Ensure the path has a trailing slash so that `join` appends
        // rather than replacing the last segment.
        if !base.path().ends_with('/') {
            base.set_path(&format!("{}/", base.path()));
        }
        base.join(path)
            .expect("joining a path to a valid base URL must succeed")
    }
}

#[derive(Clone, Debug)]
pub struct AdsCacheConfig {
    pub db_path: String,
    pub default_cache_ttl_seconds: Option<u64>,
    pub max_size_mib: Option<u64>,
}

#[cfg(test)]
mod tests {
    use url::Host;

    use super::*;

    #[test]
    fn prod_endpoint_parses_and_is_expected() {
        let url = Environment::Prod.into_url("ads");

        assert_eq!(url.as_str(), "https://ads.mozilla.org/v1/ads");

        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host(), Some(Host::Domain("ads.mozilla.org")));
        assert_eq!(url.path(), "/v1/ads");
    }
}
