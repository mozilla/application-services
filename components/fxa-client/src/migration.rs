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

/// The current state migration from legacy sign-in data.
///
/// This enum distinguishes the different states of a potential in-flight
/// migration from legacy sign-in data. A value other than [`None`](MigrationState::None)
/// indicates that there was a previously-failed migration that should be
/// retried.
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
#[derive(Debug)]
pub struct FxAMigrationResult {
    /// The time taken to migrate, in milliseconds.
    ///
    /// Note that this is a signed integer, for compatibility with languages
    /// that do not have unsigned integers.
    pub total_duration: i64,
}
