/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! # Migration Support Methods
//!
//! Some applications may have existing signed-in account state from a bespoke implementation
//! of the Firefox Accounts signin protocol, but want to move to using this component in order
//! to reduce maintenance costs.
//!
//! The sign-in state for a legacy FxA integration would typically consist of a session token
//! and a pair of cryptographic keys used for accessing Firefox Sync. The methods in this section
//! can be used to help migrate from such legacy state into state that's suitable for use with
//! this component.

use error_support::handle_error;
use crate::{ApiResult, FirefoxAccount, Error};

impl FirefoxAccount {
    /// Sign in by using legacy session-token state.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// When migrating to use the FxA client component, create a [`FirefoxAccount`] instance
    /// and then pass any legacy sign-in state to this method. It will attempt to use the
    /// session token to bootstrap a full internal state of OAuth tokens, and will store the
    /// provided credentials internally in case it needs to retry after e.g. a network failure.
    ///
    /// # Arguments
    ///
    ///    - `session_token` - the session token from legacy sign-in state
    ///    - `k_sync` - the Firefox Sync encryption key from legacy sign-in state
    ///    - `k_xcs` - the Firefox Sync "X-Client-State: value from legacy sign-in state
    ///    - `copy_session_token` - if true, copy the given session token rather than using it directly
    ///
    /// # Notes
    ///
    ///    - If successful, this method will return an [`FxAMigrationResult`] with some statistics
    ///      about the migration process.
    ///    - If unsuccessful this method will throw an error, but you may be able to retry the
    ///      migration again at a later time.
    ///    - Use [is_in_migration_state](FirefoxAccount::is_in_migration_state) to check whether the
    ///      persisted account state includes a a pending migration that can be retried.
    #[handle_error(Error)]
    pub fn migrate_from_session_token(
        &self,
        session_token: &str,
        k_sync: &str,
        k_xcs: &str,
        copy_session_token: bool,
    ) -> ApiResult<FxAMigrationResult> {
        Ok(self.internal.lock().unwrap().migrate_from_session_token(
            session_token,
            k_sync,
            k_xcs,
            copy_session_token,
        )?)
    }

    /// Retry a previously failed migration from legacy session-token state.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// If an earlier call to [`migrate_from_session_token`](FirefoxAccount::migrate_from_session_token)
    /// failed, it may have stored the provided state for retrying at a later time. Call this method
    /// in order to execute such a retry.
    #[handle_error(Error)]
    pub fn retry_migrate_from_session_token(&self) -> ApiResult<FxAMigrationResult> {
        Ok(self.internal.lock().unwrap().try_migration()?)
    }

    /// Check for a previously failed migration from legacy session-token state.
    ///
    /// If an earlier call to [`migrate_from_session_token`](FirefoxAccount::migrate_from_session_token)
    /// failed, it may have stored the provided state for retrying at a later time. Call this method
    /// in check whether such state exists, then retry at an appropriate time.
    pub fn is_in_migration_state(&self) -> MigrationState {
        self.internal.lock().unwrap().is_in_migration_state()
    }
}

/// The current state migration from legacy sign-in data.
///
/// This enum distinguishes the different states of a potential in-flight
/// migration from legacy sign-in data. A value other than [`None`](MigrationState::None)
/// indicates that there was a previously-failed migration that should be
/// retried.
///
pub enum MigrationState {
    /// No in-flight migration.
    None,
    /// An in-flight migration that will copy the sessionToken.
    CopySessionToken,
    /// An in-flight migration that will re-use the sessionToken.
    ReuseSessionToken,
}

/// Statistics about the completion of a migration from legacy sign-in data.
///
/// Applications migrating from legacy sign-in data would typically want to
/// report telemetry about whether and how that succeeded, and can use the
/// results reported in this struct to help do so.
///
#[derive(Debug)]
pub struct FxAMigrationResult {
    /// The time taken to migrate, in milliseconds.
    ///
    /// Note that this is a signed integer, for compatibility with languages
    /// that do not have unsigned integers.
    pub total_duration: i64,
}
