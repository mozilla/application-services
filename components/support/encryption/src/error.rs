/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub type Result<T> = std::result::Result<T, Error>;
// Functions which are part of the public API should use this Result.
pub type ApiResult<T> = std::result::Result<T, LoginsApiError>;

pub use error_support::{breadcrumb, handle_error, report_error};
pub use error_support::{debug, error, info, trace, warn};

use error_support::{ErrorHandling, GetErrorHandling};
use jwcrypto::JwCryptoError;

// Errors we return via the public interface.
#[derive(Debug, thiserror::Error)]
pub enum LoginsApiError {
    #[error("NSS not initialized")]
    NSSUninitialized,

    #[error("NSS error during authentication: {reason}")]
    NSSAuthenticationError { reason: String },

    #[error("error during authentication: {reason}")]
    AuthenticationError { reason: String },

    #[error("authentication cancelled")]
    AuthenticationCanceled,

    #[error("Encryption key is missing.")]
    MissingKey,

    #[error("Encryption key is not valid.")]
    InvalidKey,

    #[error("encryption failed: {reason}")]
    EncryptionFailed { reason: String },

    #[error("decryption failed: {reason}")]
    DecryptionFailed { reason: String },

    #[error("{reason}")]
    Interrupted { reason: String },

    #[error("Unexpected Error: {reason}")]
    UnexpectedLoginsApiError { reason: String },
}

/// Logins error type
/// These are "internal" errors used by the implementation. This error type
/// is never returned to the consumer.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Database is closed")]
    DatabaseClosed,

    // Fennec import only works on empty logins tables.
    #[error("The logins tables are not empty")]
    NonEmptyTable,

    #[error("encryption failed: {0:?}")]
    EncryptionFailed(String),

    #[error("decryption failed: {0:?}")]
    DecryptionFailed(String),

    #[error("CryptoError({0})")]
    CryptoError(#[from] JwCryptoError),

    #[error("IOError: {0}")]
    IOError(#[from] std::io::Error),
}

// Define how our internal errors are handled and converted to external errors
// See `support/error/README.md` for how this works, especially the warning about PII.
impl GetErrorHandling for Error {
    type ExternalError = LoginsApiError;

    fn get_error_handling(&self) -> ErrorHandling<Self::ExternalError> {
        match self {
            // Unexpected errors that we report to Sentry.  We should watch the reports for these
            // and do one or more of these things if we see them:
            //   - Fix the underlying issue
            //   - Add breadcrumbs or other context to help uncover the issue
            //   - Decide that these are expected errors and move them to the above case
            _ => ErrorHandling::convert(LoginsApiError::UnexpectedLoginsApiError {
                reason: self.to_string(),
            })
            .report_error("logins-unexpected"),
        }
    }
}

// The bridged sync engine (`sync::bridge`) deals in `anyhow::Result`, as that's
// what the `sync15` BridgedEngine traits use. This lets UniFFI map those errors
// onto our public error type when the bridge methods are exposed via the UDL.
impl From<anyhow::Error> for LoginsApiError {
    fn from(value: anyhow::Error) -> Self {
        LoginsApiError::UnexpectedLoginsApiError {
            reason: value.to_string(),
        }
    }
}

impl From<uniffi::UnexpectedUniFFICallbackError> for LoginsApiError {
    fn from(error: uniffi::UnexpectedUniFFICallbackError) -> Self {
        LoginsApiError::UnexpectedLoginsApiError {
            reason: error.to_string(),
        }
    }
}
