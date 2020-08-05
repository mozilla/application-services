/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Not implemented yet!!!
//! This is where the error definitions can go
//! TODO: Implement proper error handling, this would include defining the error enum,
//! impl std::error::Error using `thiserror` and ensuring all errors are handled appropriately

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid")]
    Invalid,
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
