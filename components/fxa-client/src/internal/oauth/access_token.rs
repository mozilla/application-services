/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::super::{scopes, util, FirefoxAccount};
use super::RefreshToken;
use crate::{error, Error, Result, ScopedKey};
use serde_derive::*;
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

// If a cached token has less than `OAUTH_MIN_TIME_LEFT` seconds left to live,
// it will be considered already expired.
const OAUTH_MIN_TIME_LEFT: u64 = 60;

fn normalize_scopes(scope: &str) -> String {
    let mut parts: Vec<&str> = scope.split_ascii_whitespace().collect();
    parts.sort_unstable();
    parts.dedup();
    parts.join(" ")
}

impl FirefoxAccount {
    /// Fetch a short-lived access token using the saved refresh token.
    /// If there is no refresh token held or if it is not authorized for some of the requested
    /// scopes, this method will error-out and a login flow will need to be initiated
    /// using `begin_oauth_flow`.
    ///
    /// * `scope` - Space-separated list of requested scopes. Order is not significant;
    ///   the cache is keyed on the normalized (sorted, deduplicated) set, so
    ///   `"a b"` and `"b a"` are treated as identical requests.
    /// * `use_cache` - optionally set to false to force a new token request.  The fetched
    ///   token will still be cached for later `get_access_token` calls.
    ///
    /// The result `AccessTokenInfo.key` will only be `Some()` when the token has a single scope
    /// and that scope has a key. If you request 'sync profile', you don't get the 'sync' key.
    ///
    /// **💾 This method may alter the persisted account state.**
    pub fn get_access_token(&mut self, scope: &str, use_cache: bool) -> Result<AccessTokenInfo> {
        let requested = normalize_scopes(scope);
        if requested.is_empty() {
            return Err(Error::IllegalState("No scopes requested."));
        }
        let requested_set: HashSet<&str> = requested.split(' ').collect();

        if use_cache {
            if let Some(oauth_info) = self.state.get_cached_access_token(&requested) {
                if oauth_info.expires_at > util::now_secs() + OAUTH_MIN_TIME_LEFT {
                    // If the cached key is missing the required sync scoped key, try to fetch it again
                    if oauth_info.check_missing_sync_scoped_key().is_ok() {
                        return Ok(oauth_info.clone());
                    }
                }
            }
        }
        let mut requested_scopes: Vec<&str> = requested_set.iter().copied().collect();
        requested_scopes.sort_unstable();
        let resp = match self.state.refresh_token() {
            Some(mut refresh_token) => {
                let missing: Vec<&str> = requested_scopes
                    .iter()
                    .copied()
                    .filter(|s| !refresh_token.scopes.contains(*s))
                    .collect();
                if !missing.is_empty() {
                    // We don't currently have all scopes - try token exchange to upgrade.
                    let exchange_resp = self.client.exchange_token_for_scope(
                        self.state.config(),
                        &refresh_token.token,
                        &missing.join(" "),
                    )?;
                    // Update state with the new refresh token that has combined scopes.
                    if let Some(new_refresh_token) = exchange_resp.refresh_token {
                        self.state.update_refresh_token(RefreshToken::new(
                            new_refresh_token,
                            exchange_resp.scope,
                        ));
                    } else {
                        // A request for a new token succeeding but without a new token is unexpected.
                        error!("successful response for a new refresh token with additional scopes, but no token was delivered");
                        // at this stage we are almost certainly still going to fail to get a token...
                    }
                    // Get the updated refresh token from state.
                    refresh_token = match self.state.refresh_token() {
                        // We had a refresh token, we must either still have the original or maybe a new one,
                        // but it's impossible for us to not have one at this point.
                        None => unreachable!("lost the refresh token"),
                        Some(token) => token,
                    };
                }
                if requested_scopes
                    .iter()
                    .all(|s| refresh_token.scopes.contains(*s))
                {
                    self.client.create_access_token_using_refresh_token(
                        self.state.config(),
                        &refresh_token.token,
                        None,
                        &requested_scopes,
                    )?
                } else {
                    // This should be impossible - if we don't have the scope we would have entered
                    // the block where we try and get it, that succeeded and we got a new refresh token,
                    // but still don't have the scope.
                    error!("New refresh token doesn't have the scopes we requested: {requested}");
                    return Err(Error::UnexpectedServerResponse);
                }
            }
            None => match self.state.session_token() {
                Some(session_token) => self.client.create_access_token_using_session_token(
                    self.state.config(),
                    session_token,
                    &requested_scopes,
                )?,
                None => return Err(Error::NoSessionToken),
            },
        };
        let since_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::IllegalState("Current date before Unix Epoch."))?;
        let expires_at = since_epoch.as_secs() + resp.expires_in;
        let key = if requested_scopes.len() == 1 {
            self.state.get_scoped_key(requested_scopes[0]).cloned()
        } else {
            None
        };
        let token_info = AccessTokenInfo {
            scope: resp.scope,
            token: resp.access_token,
            key,
            expires_at,
        };
        self.state
            .add_cached_access_token(&requested, token_info.clone());
        token_info.check_missing_sync_scoped_key()?;
        Ok(token_info)
    }

    /// **💾 This method may alter the persisted account state.**
    pub fn clear_access_token_cache(&mut self) {
        self.state.clear_access_token_cache();
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AccessTokenInfo {
    pub scope: String,
    pub token: String,
    pub key: Option<ScopedKey>,
    pub expires_at: u64, // seconds since epoch
}

impl AccessTokenInfo {
    pub fn check_missing_sync_scoped_key(&self) -> Result<()> {
        let mut parts = self.scope.split_ascii_whitespace();
        let first = parts.next();
        let is_sole_old_sync = first == Some(scopes::OLD_SYNC) && parts.next().is_none();
        if is_sole_old_sync && self.key.is_none() {
            Err(Error::SyncScopedKeyMissingInServerResponse)
        } else {
            Ok(())
        }
    }
}

impl TryFrom<AccessTokenInfo> for crate::AccessTokenInfo {
    type Error = Error;
    fn try_from(info: AccessTokenInfo) -> Result<Self> {
        Ok(crate::AccessTokenInfo {
            scope: info.scope,
            token: info.token,
            key: info.key,
            expires_at: info.expires_at.try_into()?,
        })
    }
}

impl std::fmt::Debug for AccessTokenInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccessTokenInfo")
            .field("scope", &self.scope)
            .field("key", &self.key)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

#[cfg(test)]
impl FirefoxAccount {
    pub fn add_cached_token(&mut self, scope: &str, token_info: AccessTokenInfo) {
        self.state.add_cached_access_token(scope, token_info);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::{config::Config, http_client::*};
    use mockall::predicate::{always, eq};
    use std::sync::Arc;

    fn make_fxa() -> FirefoxAccount {
        FirefoxAccount::with_config(Config::stable_dev("12345678", "https://foo.bar"))
    }

    fn token_info(scope: &str) -> AccessTokenInfo {
        AccessTokenInfo {
            scope: scope.to_owned(),
            token: "tok".to_owned(),
            key: None,
            expires_at: u64::MAX / 2,
        }
    }

    fn token_response(scope: &str) -> OAuthTokenResponse {
        OAuthTokenResponse {
            keys_jwe: None,
            refresh_token: None,
            session_token: None,
            expires_in: 3600,
            scope: scope.to_owned(),
            access_token: "at".to_owned(),
        }
    }

    fn seed_refresh_token(fxa: &mut FirefoxAccount, token: &str, scopes: &[&str]) {
        fxa.state.force_refresh_token(RefreshToken {
            token: token.to_owned(),
            scopes: scopes.iter().map(|s| s.to_string()).collect(),
        });
    }

    fn mock_scoped_key() -> crate::ScopedKey {
        crate::ScopedKey {
            kty: "oct".to_string(),
            scope: scopes::OLD_SYNC.to_string(),
            k: "k".to_string(),
            kid: "kid".to_string(),
        }
    }

    #[test]
    fn test_gat_empty_scope_errors() {
        nss::ensure_initialized();
        let mut fxa = make_fxa();
        assert!(matches!(
            fxa.get_access_token("", true),
            Err(Error::IllegalState(_))
        ));
        assert!(matches!(
            fxa.get_access_token("  ", true),
            Err(Error::IllegalState(_))
        ));
    }

    #[test]
    fn test_gat_no_tokens_errors() {
        nss::ensure_initialized();
        let mut fxa = make_fxa();
        assert!(matches!(
            fxa.get_access_token("profile", false),
            Err(Error::NoSessionToken)
        ));
    }

    #[test]
    fn test_gat_cache_hit() {
        nss::ensure_initialized();
        let mut fxa = make_fxa();
        fxa.add_cached_token("profile", token_info("profile"));
        let client = MockFxAClient::new(); // no expectations — asserts zero calls
        fxa.set_client(Arc::new(client));
        assert_eq!(fxa.get_access_token("profile", true).unwrap().token, "tok");
    }

    #[test]
    fn test_gat_cache_hit_order_insensitive() {
        nss::ensure_initialized();
        let mut fxa = make_fxa();
        fxa.add_cached_token("a b", token_info("a b")); // cached under normalized key
        let client = MockFxAClient::new();
        fxa.set_client(Arc::new(client));
        assert_eq!(fxa.get_access_token("b a", true).unwrap().token, "tok");
    }

    #[test]
    fn test_gat_single_scope_from_refresh_token() {
        nss::ensure_initialized();
        let mut fxa = make_fxa();
        seed_refresh_token(&mut fxa, "rt", &["profile"]);
        let mut client = MockFxAClient::new();
        client
            .expect_create_access_token_using_refresh_token()
            .with(always(), eq("rt"), always(), always())
            .times(1)
            .returning(|_, _, _, _| Ok(token_response("profile")));
        fxa.set_client(Arc::new(client));
        let info = fxa.get_access_token("profile", false).unwrap();
        assert_eq!(info.scope, "profile");
        assert!(info.key.is_none());
    }

    #[test]
    fn test_gat_single_scope_exchange() {
        nss::ensure_initialized();
        let mut fxa = make_fxa();
        seed_refresh_token(&mut fxa, "rt", &["profile"]);
        let mut client = MockFxAClient::new();
        client
            .expect_exchange_token_for_scope()
            .with(always(), eq("rt"), eq("sync"))
            .times(1)
            .returning(|_, _, _| {
                Ok(OAuthTokenResponse {
                    refresh_token: Some("rt2".to_string()),
                    scope: "profile sync".to_string(),
                    ..token_response("sync")
                })
            });
        client
            .expect_create_access_token_using_refresh_token()
            .times(1)
            .returning(|_, _, _, _| Ok(token_response("sync")));
        fxa.set_client(Arc::new(client));
        fxa.get_access_token("sync", false).unwrap();
        assert!(fxa.state.refresh_token().unwrap().scopes.contains("sync"));
    }

    #[test]
    fn test_gat_old_sync_key_populated() {
        nss::ensure_initialized();
        let mut fxa = make_fxa();
        seed_refresh_token(&mut fxa, "rt", &[scopes::OLD_SYNC]);
        fxa.state
            .insert_scoped_key(scopes::OLD_SYNC, mock_scoped_key());
        let mut client = MockFxAClient::new();
        client
            .expect_create_access_token_using_refresh_token()
            .times(1)
            .returning(|_, _, _, _| Ok(token_response(scopes::OLD_SYNC)));
        fxa.set_client(Arc::new(client));
        assert!(fxa
            .get_access_token(scopes::OLD_SYNC, false)
            .unwrap()
            .key
            .is_some());
    }

    #[test]
    fn test_gat_old_sync_missing_key_errors() {
        nss::ensure_initialized();
        let mut fxa = make_fxa();
        seed_refresh_token(&mut fxa, "rt", &[scopes::OLD_SYNC]);
        let mut client = MockFxAClient::new();
        client
            .expect_create_access_token_using_refresh_token()
            .times(1)
            .returning(|_, _, _, _| Ok(token_response(scopes::OLD_SYNC)));
        fxa.set_client(Arc::new(client));
        assert!(matches!(
            fxa.get_access_token(scopes::OLD_SYNC, false),
            Err(Error::SyncScopedKeyMissingInServerResponse)
        ));
    }

    #[test]
    fn test_gat_multi_scope_from_refresh_token() {
        nss::ensure_initialized();
        let mut fxa = make_fxa();
        seed_refresh_token(&mut fxa, "rt", &["profile", "sync"]);
        let mut client = MockFxAClient::new();
        client
            .expect_create_access_token_using_refresh_token()
            .times(1)
            .returning(|_, _, _, _| Ok(token_response("profile sync")));
        fxa.set_client(Arc::new(client));
        let info = fxa.get_access_token("sync profile", false).unwrap();
        assert!(info.key.is_none());
        // cached under the normalized key
        assert!(fxa.state.get_cached_access_token("profile sync").is_some());
    }

    #[test]
    fn test_gat_multi_scope_exchange_missing() {
        nss::ensure_initialized();
        let mut fxa = make_fxa();
        seed_refresh_token(&mut fxa, "rt", &["profile"]);
        let mut client = MockFxAClient::new();
        // both missing scopes passed in a single exchange call (sorted)
        client
            .expect_exchange_token_for_scope()
            .with(always(), eq("rt"), eq("newscope sync"))
            .times(1)
            .returning(|_, _, _| {
                Ok(OAuthTokenResponse {
                    refresh_token: Some("rt2".to_string()),
                    scope: "newscope profile sync".to_string(),
                    ..token_response("newscope sync")
                })
            });
        client
            .expect_create_access_token_using_refresh_token()
            .times(1)
            .returning(|_, _, _, _| Ok(token_response("newscope profile sync")));
        fxa.set_client(Arc::new(client));
        fxa.get_access_token("sync profile newscope", false)
            .unwrap();
    }

    #[test]
    fn test_gat_multi_scope_session_token() {
        nss::ensure_initialized();
        let mut fxa = make_fxa();
        fxa.set_session_token("st");
        let mut client = MockFxAClient::new();
        client
            .expect_create_access_token_using_session_token()
            .times(1)
            .returning(|_, _, _| Ok(token_response("a b")));
        fxa.set_client(Arc::new(client));
        fxa.get_access_token("b a", false).unwrap();
    }

    #[test]
    fn test_gat_multi_scope_old_sync_key_is_none() {
        nss::ensure_initialized();
        let mut fxa = make_fxa();
        let combined = format!("{} profile", scopes::OLD_SYNC);
        seed_refresh_token(&mut fxa, "rt", &[scopes::OLD_SYNC, "profile"]);
        fxa.state
            .insert_scoped_key(scopes::OLD_SYNC, mock_scoped_key());
        let mut client = MockFxAClient::new();
        client
            .expect_create_access_token_using_refresh_token()
            .times(1)
            .returning(move |_, _, _, _| Ok(token_response(&combined)));
        fxa.set_client(Arc::new(client));
        // key must be None even though OLD_SYNC is among the requested scopes
        let info = fxa
            .get_access_token(&format!("profile {}", scopes::OLD_SYNC), false)
            .unwrap();
        assert!(info.key.is_none());
    }

    #[test]
    fn test_gat_duplicate_scopes_deduped() {
        nss::ensure_initialized();
        let mut fxa = make_fxa();
        seed_refresh_token(&mut fxa, "rt", &["profile"]);
        let mut client = MockFxAClient::new();
        // "profile profile" normalizes to "profile" — treated as single scope
        client
            .expect_create_access_token_using_refresh_token()
            .times(1)
            .returning(|_, _, _, _| Ok(token_response("profile")));
        fxa.set_client(Arc::new(client));
        fxa.get_access_token("profile profile", false).unwrap();
        // cache key is "profile", so a subsequent use_cache=true call is a hit
        let client2 = MockFxAClient::new();
        fxa.set_client(Arc::new(client2));
        fxa.get_access_token("profile", true).unwrap();
    }
}
