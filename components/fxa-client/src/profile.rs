/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub use crate::http_client::ProfileResponse as Profile;
use crate::{errors::*, scopes, util, CachedResponse, FirefoxAccount};

// A cached profile response is considered fresh for `PROFILE_FRESHNESS_THRESHOLD` ms.
const PROFILE_FRESHNESS_THRESHOLD: u64 = 120_000; // 2 minutes

impl FirefoxAccount {
    /// Fetch the profile for the user.
    /// This method will error-out if the `profile` scope is not
    /// authorized for the current refresh token or or if we do
    /// not have a valid refresh token.
    ///
    /// * `ignore_cache` - If set to true, bypass the in-memory cache
    /// and fetch the entire profile data from the server.
    pub fn get_profile(&mut self, ignore_cache: bool) -> Result<Profile> {
        let profile_access_token = self.get_access_token(scopes::PROFILE)?.token;
        let mut etag = None;
        if let Some(ref cached_profile) = self.profile_cache {
            if !ignore_cache && util::now() < cached_profile.cached_at + PROFILE_FRESHNESS_THRESHOLD
            {
                return Ok(cached_profile.response.clone());
            }
            etag = Some(cached_profile.etag.clone());
        }
        match self
            .client
            .profile(&self.state.config, &profile_access_token, etag)?
        {
            Some(response_and_etag) => {
                if let Some(etag) = response_and_etag.etag {
                    self.profile_cache = Some(CachedResponse {
                        response: response_and_etag.response.clone(),
                        cached_at: util::now(),
                        etag,
                    });
                }
                Ok(response_and_etag.response)
            }
            None => match self.profile_cache {
                Some(ref cached_profile) => Ok(cached_profile.response.clone()),
                None => Err(ErrorKind::UnrecoverableServerError(
                    "Got a 304 without having sent an eTag.",
                )
                .into()),
            },
        }
    }
}

#[cfg(not(feature = "browserid"))] // Otherwise gotta impl FxABrowserIDClient too...
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{http_client::*, oauth::AccessTokenInfo, Config};
    use std::{collections::HashMap, sync::Arc};

    struct FakeClient {
        pub is_success: bool, // Can't clone Result<T, error>.
        pub profile_response: Option<ResponseAndETag<ProfileResponse>>,
    }

    impl FirefoxAccount {
        fn add_token(&mut self, scope: &str, token_info: AccessTokenInfo) {
            self.access_token_cache
                .insert(scope.to_string(), token_info);
        }
    }

    impl FxAClient for FakeClient {
        fn oauth_token_with_code(
            &self,
            _: &Config,
            _: &str,
            _: &str,
        ) -> Result<OAuthTokenResponse> {
            unimplemented!()
        }
        fn oauth_token_with_refresh_token(
            &self,
            _: &Config,
            _: &str,
            _: &[&str],
        ) -> Result<OAuthTokenResponse> {
            unimplemented!()
        }
        fn refresh_token_with_session_token(
            &self,
            _: &Config,
            _: &[u8],
            _: &[&str],
        ) -> Result<OAuthTokenResponse> {
            unimplemented!()
        }
        fn destroy_oauth_token(&self, _config: &Config, _token: &str) -> Result<()> {
            unimplemented!()
        }
        fn scoped_key_data(
            &self,
            _: &Config,
            _: &[u8],
            _: &str,
        ) -> Result<HashMap<String, ScopedKeyDataResponse>> {
            unimplemented!()
        }
        fn profile(
            &self,
            _: &Config,
            _: &str,
            _: Option<String>,
        ) -> Result<Option<ResponseAndETag<ProfileResponse>>> {
            if self.is_success {
                Ok(self.profile_response.clone())
            } else {
                panic!("Not implemented yet")
            }
        }
    }

    #[test]
    fn test_fetch_profile() {
        let mut fxa =
            FirefoxAccount::new("https://stable.dev.lcip.org", "12345678", "https://foo.bar");

        fxa.add_token(
            "profile",
            AccessTokenInfo {
                scope: "profile".to_string(),
                token: "toktok".to_string(),
                key: None,
                expires_at: u64::max_value(),
            },
        );

        let client = Arc::new(FakeClient {
            is_success: true,
            profile_response: Some(ResponseAndETag {
                response: ProfileResponse {
                    uid: "12345ab".to_string(),
                    email: "foo@bar.com".to_string(),
                    locale: "fr-FR".to_string(),
                    display_name: None,
                    avatar: "https://foo.avatar".to_string(),
                    avatar_default: true,
                    amr_values: vec![],
                    two_factor_authentication: false,
                },
                etag: None,
            }),
        });
        fxa.set_client(client);

        let p = fxa.get_profile(false).unwrap();
        assert_eq!(p.email, "foo@bar.com");
    }
}
