/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// TODO: this is (IMO) useful and was dropped from `failure`, consider moving it
// into `error_support`.
macro_rules! throw {
    ($e:expr) => {
        return Err(Into::into($e));
    };
}

#[derive(Debug, thiserror::Error)]
pub enum ErrorKind {
    #[error("Invalid login: {0}")]
    InvalidLogin(InvalidLogin),

    #[error("The `sync_status` column in DB has an illegal value: {0}")]
    BadSyncStatus(u8),

    #[error("A duplicate GUID is present: {0:?}")]
    DuplicateGuid(String),

    #[error("No record with guid exists (when one was required): {0:?}")]
    NoSuchRecord(String),

    // Fennec import only works on empty logins tables.
    #[error("The logins tables are not empty")]
    NonEmptyTable,

    #[error("The provided salt is invalid")]
    InvalidSalt,

    #[error("Error synchronizing: {0}")]
    SyncAdapterError(#[from] sync15::Error),

    #[error("Error parsing JSON data: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Error executing SQL: {0}")]
    SqlError(#[from] rusqlite::Error),

    #[error("Error parsing URL: {0}")]
    UrlParseError(#[from] url::ParseError),

    #[error("{0}")]
    Interrupted(#[from] interrupt_support::Interrupted),
}

error_support::define_error! {
    ErrorKind {
        (SyncAdapterError, sync15::Error),
        (JsonError, serde_json::Error),
        (UrlParseError, url::ParseError),
        (SqlError, rusqlite::Error),
        (InvalidLogin, InvalidLogin),
        (Interrupted, interrupt_support::Interrupted),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InvalidLogin {
    // EmptyOrigin error occurs when the login's hostname field is empty.
    #[error("Origin is empty")]
    EmptyOrigin,
    #[error("Password is empty")]
    EmptyPassword,
    #[error("Login already exists")]
    DuplicateLogin,
    #[error("Both `formSubmitUrl` and `httpRealm` are present")]
    BothTargets,
    #[error("Neither `formSubmitUrl` or `httpRealm` are present")]
    NoTarget,
    #[error("Login has illegal field: {field_info}")]
    IllegalFieldValue { field_info: String },
}

impl Error {
    // Get a short textual label identifying the type of error that occurred,
    // but without including any potentially-sensitive information.
    pub fn label(&self) -> &'static str {
        match self.kind() {
            ErrorKind::BadSyncStatus(_) => "BadSyncStatus",
            ErrorKind::DuplicateGuid(_) => "DuplicateGuid",
            ErrorKind::NoSuchRecord(_) => "NoSuchRecord",
            ErrorKind::NonEmptyTable => "NonEmptyTable",
            ErrorKind::InvalidSalt => "InvalidSalt",
            ErrorKind::SyncAdapterError(_) => "SyncAdapterError",
            ErrorKind::JsonError(_) => "JsonError",
            ErrorKind::UrlParseError(_) => "UrlParseError",
            ErrorKind::SqlError(_) => "SqlError",
            ErrorKind::Interrupted(_) => "Interrupted",
            ErrorKind::InvalidLogin(desc) => match desc {
                InvalidLogin::EmptyOrigin => "InvalidLogin::EmptyOrigin",
                InvalidLogin::EmptyPassword => "InvalidLogin::EmptyPassword",
                InvalidLogin::DuplicateLogin => "InvalidLogin::DuplicateLogin",
                InvalidLogin::BothTargets => "InvalidLogin::BothTargets",
                InvalidLogin::NoTarget => "InvalidLogin::NoTarget",
                InvalidLogin::IllegalFieldValue { .. } => "InvalidLogin::IllegalFieldValue",
            },
        }
    }
}

// Originally exposed manually via LoginsStorageException.kt.
#[derive(Debug, thiserror::Error)]
pub enum LoginsStorageError {
    #[error("Unexpected error: {0}")]
    UnexpectedLoginsStorageError(String),

    /// This indicates that the sync authentication is invalid, likely due to having
    /// expired.
    #[error("SyncAuthInvalid error: {0}")]
    SyncAuthInvalid(String),

    /// This is thrown if `lock()`/`unlock()` pairs don't match up.
    // NOTE: This can be removed once we drop sqlcipher
    #[error("MismatchedLock error: {0}")]
    MismatchedLock(&'static str),

    /// This is thrown if `update()` is performed with a record whose ID
    /// does not exist.
    #[error("NoSuchRecord error: {0}")]
    NoSuchRecord(String),

    /// This is thrown if `add()` is given a record that has an ID, and
    /// that ID does not exist.
    #[error("IdCollision error: {0}")]
    IdCollision(String),

    // This is thrown on attempts to insert or update a record so that it
    // is no longer valid. See [InvalidLoginReason] for a list of reasons
    // a record may be considered invalid
    #[error("{0}")]
    InvalidRecord(String, InvalidLoginReason),

    /// This error is emitted in two cases:
    /// 1. An incorrect key is used to to open the login database
    /// 2. The file at the path specified is not a sqlite database.
    /// NOTE: Dropping sqlcipher means we will drop (1), so should rename it
    #[error("InvalidKey error: {0}")]
    InvalidKey(String),

    /// This error is emitted if a request to a sync server failed.
    /// We can probably kill this? The sync manager is what cares about this.
    #[error("RequestFailed error: {0}")]
    RequestFailed(String),

    /// This error is emitted if a sync or other operation is interrupted.
    #[error("Interrupted error: {0}")]
    Interrupted(String),
}

/**
 * A reason a login may be invalid
 */
#[derive(Debug)]
pub enum InvalidLoginReason {
    /// Origins may not be empty
    EmptyOrigin,
    /// Passwords may not be empty
    EmptyPassword,
    /// The login already exists
    DuplicateLogin,
    /// Both `httpRealm` and `formSubmitUrl` are non-null
    BothTargets,
    /// Both `httpRealm` and `formSubmitUrl` are null
    NoTarget,
    /// Login has illegal field
    IllegalFieldValue,
}

// A port of the error conversion stuff that was in ffi.rs - it turns our
// "internal" errors into "public" ones.
impl From<Error> for LoginsStorageError {
    fn from(e: Error) -> LoginsStorageError {
        use sync15::ErrorKind as Sync15ErrorKind;

        let label = e.label().to_string();
        let kind = e.kind();
        match kind {
            ErrorKind::SyncAdapterError(e) => {
                log::error!("Sync error {:?}", e);
                match e.kind() {
                    Sync15ErrorKind::TokenserverHttpError(401)
                    | Sync15ErrorKind::BadKeyLength(..) => {
                        LoginsStorageError::SyncAuthInvalid(label)
                    }
                    Sync15ErrorKind::RequestError(_) => LoginsStorageError::RequestFailed(label),
                    _ => LoginsStorageError::UnexpectedLoginsStorageError(label),
                }
            }
            ErrorKind::DuplicateGuid(id) => {
                log::error!("Guid already exists: {}", id);
                LoginsStorageError::IdCollision(label)
            }
            ErrorKind::NoSuchRecord(id) => {
                log::error!("No record exists with id {}", id);
                LoginsStorageError::NoSuchRecord(label)
            }
            ErrorKind::InvalidLogin(desc) => {
                log::error!("Invalid login: {}", desc);
                match desc {
                    InvalidLogin::EmptyOrigin => {
                        LoginsStorageError::InvalidRecord(label, InvalidLoginReason::EmptyOrigin)
                    }
                    InvalidLogin::EmptyPassword => {
                        LoginsStorageError::InvalidRecord(label, InvalidLoginReason::EmptyPassword)
                    }
                    InvalidLogin::DuplicateLogin => {
                        LoginsStorageError::InvalidRecord(label, InvalidLoginReason::DuplicateLogin)
                    }
                    InvalidLogin::BothTargets => {
                        LoginsStorageError::InvalidRecord(label, InvalidLoginReason::BothTargets)
                    }
                    InvalidLogin::NoTarget => {
                        LoginsStorageError::InvalidRecord(label, InvalidLoginReason::NoTarget)
                    }
                    InvalidLogin::IllegalFieldValue { .. } => LoginsStorageError::InvalidRecord(
                        label,
                        InvalidLoginReason::IllegalFieldValue,
                    ),
                }
            }
            // We can't destructure `err` without bringing in the libsqlite3_sys crate
            // (and I'd really rather not) so we can't put this in the match.
            ErrorKind::SqlError(rusqlite::Error::SqliteFailure(err, _))
                if err.code == rusqlite::ErrorCode::NotADatabase =>
            {
                log::error!("Not a database / invalid key error");
                LoginsStorageError::InvalidKey(label)
            }

            ErrorKind::SqlError(rusqlite::Error::SqliteFailure(err, _))
                if err.code == rusqlite::ErrorCode::OperationInterrupted =>
            {
                log::warn!("Operation interrupted (SQL)");
                LoginsStorageError::Interrupted(label)
            }

            ErrorKind::Interrupted(_) => {
                log::warn!("Operation interrupted (Outside SQL)");
                LoginsStorageError::Interrupted(label)
            }

            ErrorKind::InvalidSalt => {
                log::error!("Invalid salt provided");
                // In the old world, this had an error code (7) but no Kotlin
                // error type, meaning it got the "base" error.
                LoginsStorageError::UnexpectedLoginsStorageError(label)
            }

            err => {
                log::error!("UnexpectedLoginsStorageError error: {:?}", err);
                LoginsStorageError::UnexpectedLoginsStorageError(label)
            }
        }
    }
}
