/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! The public errors carry nothing that has to be kept out of an error report,
//! so they double as the internal ones: the [`GetErrorHandling`] impls only
//! pick what gets logged and what gets reported, and `#[handle_error]` applies
//! that at the FFI boundary. See `components/support/error/README.md`.

use error_support::{ErrorHandling, GetErrorHandling};
use thiserror::Error;

/// Internal: the embedder sees these folded into [`InitError`].
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub(crate) enum ParseError {
    #[error("malformed containers data: {0}")]
    Malformed(String),
    #[error("unsupported containers data version: {0}")]
    UnsupportedVersion(u32),
}

impl From<serde_json::Error> for ParseError {
    fn from(error: serde_json::Error) -> Self {
        ParseError::Malformed(error.to_string())
    }
}

/// Why a store could not be opened.
#[derive(Clone, Debug, PartialEq, Eq, Error, uniffi::Error)]
#[non_exhaustive]
pub enum InitError {
    /// Carried as text rather than as a `serde_json::Error`, to keep the
    /// serialization library out of the public surface.
    #[error("malformed containers data: {reason}")]
    Malformed { reason: String },
    #[error("unsupported containers data version: {version}")]
    UnsupportedVersion { version: u32 },
    #[error("unknown container icon in seed: {icon}")]
    InvalidSeedIcon { icon: String },
    #[error("unknown container color in seed: {color}")]
    InvalidSeedColor { color: String },
}

impl From<ParseError> for InitError {
    fn from(error: ParseError) -> Self {
        match error {
            ParseError::Malformed(reason) => InitError::Malformed { reason },
            ParseError::UnsupportedVersion(version) => InitError::UnsupportedVersion { version },
        }
    }
}

impl GetErrorHandling for InitError {
    type ExternalError = Self;

    fn get_error_handling(&self) -> ErrorHandling<Self> {
        match self {
            // Unreadable data costs the user their containers, so we want to
            // hear about it.
            Self::Malformed { .. } => {
                ErrorHandling::convert(self.clone()).report_error("containers-malformed-data")
            }

            // Version 1 predates every migration path: an old enough profile,
            // not a bug.
            Self::UnsupportedVersion { version: 1 } => {
                ErrorHandling::convert(self.clone()).log_warning()
            }

            // Any other unreadable version means a downgrade, or a migration we
            // should have had.
            Self::UnsupportedVersion { .. } => {
                ErrorHandling::convert(self.clone()).report_error("containers-unsupported-version")
            }

            // The seed is the embedder's to get right.
            Self::InvalidSeedIcon { .. } | Self::InvalidSeedColor { .. } => {
                ErrorHandling::convert(self.clone()).log_warning()
            }
        }
    }
}

/// A mutation that could not be applied. The store is left untouched.
#[derive(Clone, Debug, PartialEq, Eq, Error, uniffi::Error)]
#[non_exhaustive]
pub enum StoreError {
    #[error("container names cannot contain only whitespace")]
    EmptyName,
    #[error("unknown container {user_context_id}")]
    NoSuchContainer { user_context_id: u32 },
    #[error("invalid site for a container association")]
    InvalidSite,
    #[error("no userContextId left to assign")]
    IdSpaceExhausted,
}

impl GetErrorHandling for StoreError {
    type ExternalError = Self;

    fn get_error_handling(&self) -> ErrorHandling<Self> {
        match self {
            // Four billion containers is not a thing, so the counter is broken.
            Self::IdSpaceExhausted => {
                ErrorHandling::convert(self.clone()).report_error("containers-id-space-exhausted")
            }

            // The rest is just rejected input.
            _ => ErrorHandling::convert(self.clone()),
        }
    }
}
