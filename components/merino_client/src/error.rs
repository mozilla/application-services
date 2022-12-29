/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub type MerinoClientResult<T> = std::result::Result<T, MerinoClientError>;

#[derive(Debug, thiserror::Error)]
pub enum MerinoClientError {
    #[error("Malformed URL: {reason}")]
    BadUrl { reason: String },

    #[error("Failed to fetch suggestions: {reason}")]
    FetchFailed { reason: String },
}
