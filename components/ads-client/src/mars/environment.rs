/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use once_cell::sync::Lazy;
use url::Url;
use url_macro::url;

static MARS_API_ENDPOINT_PROD: Lazy<Url> = Lazy::new(|| url!("https://ads.mozilla.org/v1/"));

static MARS_API_ENDPOINT_STAGING: Lazy<Url> = Lazy::new(|| url!("https://ads.allizom.org/v1/"));

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum Environment {
    #[default]
    Prod,
    Staging,
    #[cfg(test)]
    Test,
}

impl Environment {
    pub fn into_url(self, path: &str) -> Url {
        let mut url = self.base_url();
        url.path_segments_mut()
            .expect("base URL must be hierarchical")
            .pop_if_empty()
            .extend(path.split('/').filter(|segment| !segment.is_empty()));
        url
    }

    fn base_url(self) -> Url {
        match self {
            Environment::Prod => MARS_API_ENDPOINT_PROD.clone(),
            Environment::Staging => MARS_API_ENDPOINT_STAGING.clone(),
            #[cfg(test)]
            Environment::Test => Url::parse(&mockito::server_url()).unwrap(),
        }
    }
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

    #[test]
    fn staging_endpoint_parses_and_is_expected() {
        let url = Environment::Staging.into_url("ads");

        assert_eq!(url.as_str(), "https://ads.allizom.org/v1/ads");

        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host(), Some(Host::Domain("ads.allizom.org")));
        assert_eq!(url.path(), "/v1/ads");
    }

    #[test]
    fn into_url_normalizes_slash() {
        assert_eq!(
            Environment::Prod.into_url("/ads"),
            Environment::Prod.into_url("ads"),
        );
        assert_eq!(
            Environment::Prod.into_url("//ads/with/extra//slashes//"),
            Environment::Prod.into_url("ads/with/extra/slashes"),
        );
    }
}
