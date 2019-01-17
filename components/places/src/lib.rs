/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub mod api;
pub mod error;
pub mod types;
// Making these all pub for now while we flesh out the API.
pub mod db;
#[cfg(feature = "ffi")]
pub mod ffi;
pub mod frecency;
pub mod hash;
pub mod history_sync;
mod match_impl;
pub mod observation;
pub mod storage;
mod util;
mod valid_guid;

pub use crate::api::apply_observation;
pub use crate::db::{PlacesDb, PlacesInterruptHandle};
pub use crate::error::*;
pub use crate::observation::VisitObservation;
pub use crate::storage::PageInfo;
pub use crate::storage::RowId;
pub use crate::types::*;
