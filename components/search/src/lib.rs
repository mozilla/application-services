/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

mod error;
pub use error::SearchApiError;

pub mod selector;
pub use selector::SearchEngineSelector;

pub type SearchApiResult<T> = std::result::Result<T, error::SearchApiError>;

uniffi::include_scaffolding!("search");
