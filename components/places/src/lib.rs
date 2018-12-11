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
extern crate sql_support;
extern crate unicode_normalization;
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

pub use api::apply_observation;
pub use db::PlacesDb;
pub use error::*;
pub use observation::VisitObservation;
pub use storage::{PageInfo, RowId};
pub use types::*;
