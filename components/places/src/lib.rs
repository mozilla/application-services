/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(unknown_lints)]
#![warn(rust_2018_idioms)]

pub mod api;
pub mod error;
pub mod types;
// Making these all pub for now while we flesh out the API.
pub mod bookmark_sync;
pub mod db;
pub mod ffi;
pub mod frecency;
pub mod hash;
pub mod history_sync;
// match_impl is pub mostly for benchmarks (which have to run as a separate pseudo-crate).
pub mod import;
pub mod match_impl;
pub mod observation;
pub mod storage;
#[cfg(test)]
mod tests;
mod util;

pub use crate::api::apply_observation;
#[cfg(test)]
pub use crate::api::places_api::test;
pub use crate::api::places_api::{get_registered_sync_engine, ConnectionType, PlacesApi};

pub use crate::db::PlacesDb;
pub use crate::error::*;
pub use crate::observation::*;
pub use crate::storage::PageInfo;
pub use crate::storage::RowId;
pub use crate::types::*;

pub use ffi::*;

uniffi::include_scaffolding!("places");
