/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use error_support::report_error;
use std::ffi::OsString;

pub type Result<T> = std::result::Result<T, LoginsError>;
pub type APIResult<T> = std::result::Result<T, LoginsStorageError>;

/// Internal logins error type, this is what we use for inside this crate
#[derive(Debug, thiserror::Error)]
pub enum LoginsError {
    // WARNING: The #[error] attributes define the string representation of the error (see
    // thiserror for details).  These strings should not contain any personally identifying
    // information.  We operate on a best-effort basis, since we can't be completely sure that
    // our dependencies don't leak PII in their error strings.  For example, `rusqlite::Error`
    // could include data from a user's database in their errors, but we've never seen that in
    // practice so we are comfortable forwading that error message in ours.
    #[error("Invalid login: {0}")]
    InvalidLogin(#[from] InvalidLogin),

    #[error("The `sync_status` column in DB has an illegal value: {0}")]
    BadSyncStatus(u8),

    #[error("No record with guid exists (when one was required): {0:?}")]
    NoSuchRecord(String),

    // Fennec import only works on empty logins tables.
    #[error("The logins tables are not empty")]
    NonEmptyTable,

    #[error("local encryption key not set")]
    EncryptionKeyMissing,

    #[error("Error synchronizing: {0}")]
    SyncAdapterError(#[from] sync15::Error),

    #[error("Error parsing JSON data: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Invalid encryption key")]
    InvalidKey,

    #[error("Error executing SQL: {0}")]
    SqlError(#[from] rusqlite::Error),

    #[error("Error parsing URL: {0}")]
    UrlParseError(#[from] url::ParseError),

    #[error("Invalid path: {0:?}")]
    InvalidPath(OsString),

    #[error("Invalid database file: {0}")]
    InvalidDatabaseFile(String),

    #[error("Crypto Error: {0}")]
    CryptoError(#[from] jwcrypto::JwCryptoError),

    #[error("{0}")]
    Interrupted(#[from] interrupt_support::Interrupted),

    #[error("IOError: {0}")]
    IOError(#[from] std::io::Error),

    #[error("Migration Error: {0}")]
    MigrationError(String),
}

/// Error::InvalidLogin subtypes
#[derive(Debug, thiserror::Error)]
pub enum InvalidLogin {
    // EmptyOrigin error occurs when the login's origin field is empty.
    #[error("Origin is empty")]
    EmptyOrigin,
    #[error("Password is empty")]
    EmptyPassword,
    #[error("Login already exists")]
    DuplicateLogin,
    #[error("Both `formActionOrigin` and `httpRealm` are present")]
    BothTargets,
    #[error("Neither `formActionOrigin` or `httpRealm` are present")]
    NoTarget,
    #[error("Login has illegal field: {field_info}")]
    IllegalFieldValue { field_info: String },
}

/// Public logins error type, we convert from `LoginsError` to `LoginsStorageError` in the
/// top-level functions that we expose via UniFFI.
///
/// `LoginsStorageError` only contains variants that are useful to the consuming app, for example:
///    - `InvalidLogin` is useful, because the app can inform the user that the login they entered
///      was invalid.
///    - `Interrupted` is useful because the app can choose to ignore these errors.
///    - `BadSyncStatus` is not useful to the app so it gets grouped into the
///      UnexpectedLoginsError.
#[derive(Debug, thiserror::Error)]
pub enum LoginsStorageError {
    // This is thrown on attempts to insert or update a record so that it
    // is no longer valid. See [InvalidLoginReason] for a list of reasons
    // a record may be considered invalid
    #[error("Invalid login: {0}")]
    InvalidRecord(InvalidLogin),

    /// This is thrown if `update()` is performed with a record whose ID
    /// does not exist.
    #[error("No record with guid exists (when one was required): {0}")]
    NoSuchRecord(String),

    /// Error encrypting/decrypting logins data
    #[error("Encryption error: {0}")]
    CryptoError(String),

    /// This indicates that the sync authentication is invalid, likely due to having
    /// expired.
    #[error("SyncAuthInvalid error: {0}")]
    SyncAuthInvalid(String),

    /// This error is emitted if a request to a sync server failed.
    ///
    /// Once iOS is using the sync manager, we can probably kill this.  Since the sync manager will
    /// then be handling the error.
    #[error("RequestFailed error: {0}")]
    RequestFailed(String),

    /// Operation was interrupted by the user
    #[error("Operation interrupted: {0}")]
    Interrupted(String),

    /// Catch-all for all other errors
    #[error("Unexpected error: {0}")]
    UnexpectedLoginsError(String),
}

impl From<LoginsError> for LoginsStorageError {
    fn from(error: LoginsError) -> LoginsStorageError {
        // We convert errors before sending them across the API boundary to the consuming
        // application, so this is a good time to report them.
        error.report();

        match error {
            LoginsError::InvalidLogin(inner) => Self::InvalidRecord(inner),
            LoginsError::NoSuchRecord(guid) => Self::NoSuchRecord(guid),
            LoginsError::CryptoError(inner) => Self::CryptoError(inner.to_string()),
            LoginsError::InvalidKey => Self::CryptoError("InvalidKey".to_string()),
            LoginsError::SyncAdapterError(ref e) => match e.kind() {
                sync15::ErrorKind::TokenserverHttpError(401)
                | sync15::ErrorKind::BadKeyLength(..) => Self::SyncAuthInvalid(error.to_string()),
                sync15::ErrorKind::RequestError(_) => Self::RequestFailed(error.to_string()),
                _ => Self::UnexpectedLoginsError(error.to_string()),
            },
            _ => Self::UnexpectedLoginsError(error.to_string()),
        }
    }
}

/// These are needed for the LoginsStore::sync() method.  Once iOS has moved to `SyncManager` they
/// can be deleted alongside that method
impl From<sync15::Error> for LoginsStorageError {
    fn from(error: sync15::Error) -> LoginsStorageError {
        LoginsError::from(error).into()
    }
}
impl From<url::ParseError> for LoginsStorageError {
    fn from(error: url::ParseError) -> LoginsStorageError {
        LoginsError::from(error).into()
    }
}

/// Needed for support the JSON serialization of import_multiple().  Maybe this can be refactored
/// to avoid this
impl From<serde_json::Error> for LoginsStorageError {
    fn from(error: serde_json::Error) -> LoginsStorageError {
        LoginsError::from(error).into()
    }
}

/// Classify errors into different categories
#[derive(Clone, Debug)]
pub enum ErrorClassification {
    /// Errors that we expect to happen regularly, like network errors or DB corruption errors.
    /// Our strategy for these errors is to eventually report them to telemetry and ensure that the
    /// counts remain relatively stable. The string value will be used to group errors when
    /// counting.
    Expected(String),
    /// Errors that we don't expect to see.  Our strategy for these errors is to report them to a
    /// Sentry-like reporting system and investigate them when they come up.  The string value will
    /// be used to group errors in the reporting system.
    Unexpected(String),
}

impl LoginsError {
    // Get a short textual label identifying the type of error that occurred, but without specific
    // data like GUIDs, SQLite error messages, etc.  This is used to group the errors in Sentry and
    // telemetry.
    pub fn classify(&self) -> ErrorClassification {
        // Convenience functions to create ErrorClassification instances
        fn unexpected(grouping: impl Into<String>) -> ErrorClassification {
            ErrorClassification::Unexpected(grouping.into())
        }
        fn expected(grouping: impl Into<String>) -> ErrorClassification {
            ErrorClassification::Expected(grouping.into())
        }

        match self {
            // TODO: The legacy code called `log::error` for these, but should we be doing that?
            // Let's decide once these are properly grouped in sentry.
            Self::SyncAdapterError(_) => unexpected("SyncError"),
            Self::NoSuchRecord(_) => unexpected("NoSuchRecord"),
            Self::CryptoError(_) => unexpected("CryptoError"),
            // Expected errors
            Self::InvalidKey => expected("InvalidKey"),
            Self::InvalidLogin(desc) => match desc {
                InvalidLogin::EmptyOrigin => expected("InvalidLogin::EmptyOrigin"),
                InvalidLogin::EmptyPassword => expected("InvalidLogin::EmptyPassword"),
                InvalidLogin::DuplicateLogin => expected("InvalidLogin::DuplicateLogin"),
                InvalidLogin::BothTargets => expected("InvalidLogin::BothTargets"),
                InvalidLogin::NoTarget => expected("InvalidLogin::NoTarget"),
                InvalidLogin::IllegalFieldValue { .. } => {
                    expected("InvalidLogin::IllegalFieldValue")
                }
            },
            Self::SqlError(rusqlite::Error::SqliteFailure(err, _))
                if err.code == rusqlite::ErrorCode::NotADatabase =>
            {
                // TODO: investigate if this still happens now that we're not using sqlcipher
                unexpected("NotADatabase")
            }
            Self::SqlError(rusqlite::Error::SqliteFailure(err, _))
                if err.code == rusqlite::ErrorCode::OperationInterrupted =>
            {
                expected("Interrupted")
            }
            Self::Interrupted(_) => expected("Interrupted"),
            // TODO: the legacy code grouped all other errors together and called `log::error` for
            // them.  We should go through these errors in Sentry and properly classify them.
            _ => unexpected("UnexpectedError"),
        }
    }

    pub fn group_name(&self) -> String {
        match self.classify() {
            ErrorClassification::Unexpected(group_name)
            | ErrorClassification::Expected(group_name) => group_name,
        }
    }

    /// Report this error to our tracking system if appropriate
    pub fn report(&self) {
        let error_string = self.to_string();
        match self.classify() {
            ErrorClassification::Unexpected(group_name) => {
                // TODO: this should be `log::error`, but that's hooked up the legacy sentry
                // reporting code so that would result in reporting the error twice.  Once the
                // legacy code is reported by `report_error!`, this should get changed to
                // `log::error`
                log::warn!("{}", error_string);
                report_error!(group_name, "{}", error_string);
            }
            ErrorClassification::Expected(_) => {
                log::warn!("{}", error_string);
                // TODO: report these to telemetry
            }
        }
    }
}
