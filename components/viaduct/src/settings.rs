/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use once_cell::sync::Lazy;
use parking_lot::RwLock;
use std::time::Duration;
use url::Url;

/// Note: reqwest allows these only to be specified per-Client. concept-fetch
/// allows these to be specified on each call to fetch. I think it's worth
/// keeping a single global reqwest::Client in the reqwest backend, to simplify
/// the way we abstract away from these.
///
/// In the future, should we need it, we might be able to add a CustomClient type
/// with custom settings. In the reqwest backend this would store a Client, and
/// in the concept-fetch backend it would only store the settings, and populate
/// things on the fly.
#[derive(Debug)]
#[non_exhaustive]
pub struct Settings {
    pub read_timeout: Option<Duration>,
    pub connect_timeout: Option<Duration>,
    pub follow_redirects: bool,
    pub use_caches: bool,
    pub default_user_agent: Option<String>,
    // For testing purposes, we allow exactly one additional Url which is
    // allowed to not be https.
    //
    // Note: this is the only setting the new backend code uses.  Once all applications have moved
    // away from the legacy backend, we can delete all other fields.
    pub addn_allowed_insecure_url: Option<Url>,
}

#[cfg(target_os = "ios")]
const TIMEOUT_DURATION: Duration = Duration::from_secs(7);

#[cfg(not(target_os = "ios"))]
const TIMEOUT_DURATION: Duration = Duration::from_secs(10);

// The singleton instance of our settings.
pub static GLOBAL_SETTINGS: Lazy<RwLock<Settings>> = Lazy::new(|| {
    RwLock::new(Settings {
        read_timeout: Some(TIMEOUT_DURATION),
        connect_timeout: Some(TIMEOUT_DURATION),
        follow_redirects: true,
        use_caches: false,
        default_user_agent: None,
        addn_allowed_insecure_url: None,
    })
});

/// Allow non-HTTPS requests to the emulator loopback URL
#[uniffi::export]
pub fn allow_android_emulator_loopback() {
    let url = url::Url::parse("http://10.0.2.2").unwrap();
    let mut settings = GLOBAL_SETTINGS.write();
    settings.addn_allowed_insecure_url = Some(url);
}

/// Set the global default user-agent
///
/// This is what's used when no user-agent is set in the `ClientSettings` and no `user-agent`
/// header is set in the Request.
#[uniffi::export]
pub fn set_global_default_user_agent(user_agent: String) {
    let mut settings = GLOBAL_SETTINGS.write();
    settings.default_user_agent = Some(user_agent);
}

/// Validate a request, respecting the `addn_allowed_insecure_url` setting.
pub fn validate_request(request: &crate::Request) -> Result<(), crate::ViaductError> {
    if request.url.scheme() != "https"
        && match request.url.host() {
            Some(url::Host::Domain(d)) => d != "localhost",
            Some(url::Host::Ipv4(addr)) => !addr.is_loopback(),
            Some(url::Host::Ipv6(addr)) => !addr.is_loopback(),
            None => true,
        }
        && {
            let settings = GLOBAL_SETTINGS.read();
            settings
                .addn_allowed_insecure_url
                .as_ref()
                .map(|url| url.host() != request.url.host() || url.scheme() != request.url.scheme())
                .unwrap_or(true)
        }
    {
        return Err(crate::ViaductError::NonTlsUrl);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_request() {
        let _https_request = crate::Request::new(
            crate::Method::Get,
            url::Url::parse("https://www.example.com").unwrap(),
        );
        assert!(validate_request(&_https_request).is_ok());

        let _http_request = crate::Request::new(
            crate::Method::Get,
            url::Url::parse("http://www.example.com").unwrap(),
        );
        assert!(validate_request(&_http_request).is_err());

        let _localhost_https_request = crate::Request::new(
            crate::Method::Get,
            url::Url::parse("https://127.0.0.1/index.html").unwrap(),
        );
        assert!(validate_request(&_localhost_https_request).is_ok());

        let _localhost_https_request_2 = crate::Request::new(
            crate::Method::Get,
            url::Url::parse("https://localhost:4242/").unwrap(),
        );
        assert!(validate_request(&_localhost_https_request_2).is_ok());

        let _localhost_http_request = crate::Request::new(
            crate::Method::Get,
            url::Url::parse("http://localhost:4242/").unwrap(),
        );
        assert!(validate_request(&_localhost_http_request).is_ok());

        let localhost_request = crate::Request::new(
            crate::Method::Get,
            url::Url::parse("localhost:4242/").unwrap(),
        );
        assert!(validate_request(&localhost_request).is_err());

        let localhost_request_shorthand_ipv6 =
            crate::Request::new(crate::Method::Get, url::Url::parse("http://[::1]").unwrap());
        assert!(validate_request(&localhost_request_shorthand_ipv6).is_ok());

        let localhost_request_ipv6 = crate::Request::new(
            crate::Method::Get,
            url::Url::parse("http://[0:0:0:0:0:0:0:1]").unwrap(),
        );
        assert!(validate_request(&localhost_request_ipv6).is_ok());
    }

    #[test]
    fn test_validate_request_addn_allowed_insecure_url() {
        let request_root = crate::Request::new(
            crate::Method::Get,
            url::Url::parse("http://anything").unwrap(),
        );
        let request = crate::Request::new(
            crate::Method::Get,
            url::Url::parse("http://anything/path").unwrap(),
        );
        // This should never be accepted.
        let request_ftp = crate::Request::new(
            crate::Method::Get,
            url::Url::parse("ftp://anything/path").unwrap(),
        );
        assert!(validate_request(&request_root).is_err());
        assert!(validate_request(&request).is_err());
        {
            let mut settings = GLOBAL_SETTINGS.write();
            settings.addn_allowed_insecure_url =
                Some(url::Url::parse("http://something-else").unwrap());
        }
        assert!(validate_request(&request_root).is_err());
        assert!(validate_request(&request).is_err());

        {
            let mut settings = GLOBAL_SETTINGS.write();
            settings.addn_allowed_insecure_url = Some(url::Url::parse("http://anything").unwrap());
        }
        assert!(validate_request(&request_root).is_ok());
        assert!(validate_request(&request).is_ok());
        assert!(validate_request(&request_ftp).is_err());
    }
}
