/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Not complete yet
//! This is where the error definitions can go
//! TODO: Implement proper error handling, this would include defining the error enum,
//! impl std::error::Error using `thiserror` and ensuring all errors are handled appropriately

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid persisted data")]
    InvalidPersistedData,
    #[error("Rkv error: {0}")]
    RkvError(rkv::StoreError),
    #[error("IO error: {0}")]
    IOError(#[from] std::io::Error),
    #[error("JSON Error: {0}")]
    JSONError(#[from] serde_json::Error),
}

// This can be replaced with #[from] in the enum definition
// once rkv::StoreError impl std::error:Error (https://github.com/mozilla/rkv/issues/188)
impl From<rkv::StoreError> for Error {
    fn from(store_error: rkv::StoreError) -> Self {
        Error::RkvError(store_error)
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
