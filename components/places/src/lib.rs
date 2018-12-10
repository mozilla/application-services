/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

extern crate sync15_adapter;

#[macro_use]
extern crate log;

#[cfg(test)]
extern crate env_logger;

extern crate failure;

extern crate unicode_segmentation;

extern crate url;

extern crate rusqlite;

extern crate serde;
#[cfg_attr(test, macro_use)]
extern crate serde_json;

#[macro_use]
extern crate serde_derive;

extern crate caseless;
extern crate unicode_normalization;
extern crate sql_support;
extern crate url_serde;
#[macro_use]
extern crate bitflags;

#[cfg(feature = "ffi")]
#[macro_use]
extern crate ffi_support;

#[macro_use]
extern crate lazy_static;

pub mod api;
pub mod error;
pub mod types;
// Making these all pub for now while we flesh out the API.
pub mod db;
pub mod storage;
pub mod hash;
pub mod frecency;
pub mod observation;
pub mod history_sync;
mod util;
#[cfg(feature = "ffi")]
pub mod ffi;
mod match_impl;
mod valid_guid;

pub use error::*;
pub use types::*;
pub use observation::VisitObservation;
pub use storage::{RowId, PageInfo};
pub use db::PlacesDb;
pub use api::apply_observation;

