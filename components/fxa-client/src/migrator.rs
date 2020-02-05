/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{error::*, scoped_keys::ScopedKey, scopes, FirefoxAccount, MigrationData};
use serde_derive::*;
use std::time::Instant;

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Default)]
pub struct FxAMigrationResult {
    pub total_duration: u128,
}

impl FirefoxAccount {
    /// Migrate from a logged-in with a sessionToken Firefox Account.
    ///
    /// * `session_token` - Hex-formatted session token.
    /// * `k_xcs` - Hex-formatted kXCS.
    /// * `k_sync` - Hex-formatted kSync.
    /// * `copy_session_token` - If true then the provided 'session_token' will be duplicated
    ///     and the resulting session will use a new session token. If false, the provided
    ///     token will be reused.
    ///
    /// This method remembers the provided token details and may persist them in the
    /// account state if it encounters a temporary failure such as a network error.
    /// Calling code is expected to store the updated state even if an error is thrown.
    ///
    /// **💾 This method alters the persisted account state.**
    pub fn migrate_from_session_token(
        &mut self,
        session_token: &str,
        k_sync: &str,
        k_xcs: &str,
        copy_session_token: bool,
    ) -> Result<FxAMigrationResult> {
        // if there is already a session token on account, we error out.
        if self.state.session_token.is_some() {
            return Err(ErrorKind::IllegalState("Session Token is already set.").into());
        }

        self.state.in_flight_migration = Some(MigrationData {
            k_sync: k_sync.to_string(),
            k_xcs: k_xcs.to_string(),
            copy_session_token,
            session_token: session_token.to_string(),
        });

        self.try_migration()
    }

    /// Check if the client is in a pending migration state
    pub fn is_in_migration_state(&self) -> bool {
        self.state.in_flight_migration.is_some()
    }

    pub fn try_migration(&mut self) -> Result<FxAMigrationResult> {
        let import_start = Instant::now();

        match self.network_migration() {
            Ok(_) => {}
            Err(err) => {
                match err.kind() {
                    ErrorKind::RemoteError {
                        code: 500..=599, ..
                    }
                    | ErrorKind::RemoteError { code: 429, .. }
                    | ErrorKind::RequestError(_) => {
                        // network errors that will allow hopefully migrate later
                        log::warn!("Network error: {:?}", err);
                        return Err(err);
                    }
                    _ => {
                        // probably will not recover

                        self.state.in_flight_migration = None;

                        return Err(err);
                    }
                };
            }
        }

        self.state.in_flight_migration = None;

        let metrics = FxAMigrationResult {
            total_duration: import_start.elapsed().as_millis(),
        };

        Ok(metrics)
    }

    fn network_migration(&mut self) -> Result<()> {
        let migration_data = match self.state.in_flight_migration {
            Some(ref data) => data.clone(),
            None => {
                return Err(ErrorKind::NoMigrationData.into());
            }
        };

        // If we need to copy the sessionToken, do that first so we can use it
        // for subsequent requests. TODO: we should store the duplicated token
        // in the account state in case we fail in later steps, but need to remember
        // the original value of `copy_session_token` if we do so.
        let migration_session_token = if migration_data.copy_session_token {
            let duplicate_session = self
                .client
                .duplicate_session(&self.state.config, &migration_data.session_token)?;

            duplicate_session.session_token
        } else {
            migration_data.session_token.to_string()
        };

        // Synthesize a scoped key from our kSync.
        // Do this before creating OAuth tokens because it doesn't have any side-effects,
        // so it's low-consequence if we fail in later steps.
        let k_sync = hex::decode(&migration_data.k_sync)?;
        let k_sync = base64::encode_config(&k_sync, base64::URL_SAFE_NO_PAD);
        let k_xcs = hex::decode(&migration_data.k_xcs)?;
        let k_xcs = base64::encode_config(&k_xcs, base64::URL_SAFE_NO_PAD);
        let scoped_key_data = self.client.scoped_key_data(
            &self.state.config,
            &migration_session_token,
            scopes::OLD_SYNC,
        )?;
        let oldsync_key_data = scoped_key_data.get(scopes::OLD_SYNC).ok_or_else(|| {
            ErrorKind::IllegalState("The session token doesn't have access to kSync!")
        })?;
        let kid = format!("{}-{}", oldsync_key_data.key_rotation_timestamp, k_xcs);
        let k_sync_scoped_key = ScopedKey {
            kty: "oct".to_string(),
            scope: scopes::OLD_SYNC.to_string(),
            k: k_sync,
            kid,
        };

        // Trade our session token for a refresh token.
        let oauth_response = self.client.oauth_tokens_from_session_token(
            &self.state.config,
            &migration_session_token,
            &[scopes::PROFILE, scopes::OLD_SYNC],
        )?;

        // Store the new tokens in the account state.
        // We do this all at one at the end to avoid leaving partial state.
        self.state.session_token = Some(migration_session_token);
        self.handle_oauth_response(oauth_response, None)?;
        self.state
            .scoped_keys
            .insert(scopes::OLD_SYNC.to_string(), k_sync_scoped_key);

        Ok(())
    }
}
