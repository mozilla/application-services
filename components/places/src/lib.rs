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

#[cfg(all(feature = "glean-sym", any(target_os = "android", target_os = "ios")))]
#[allow(clippy::all)] // Don't lint generated code.
pub mod glean_metrics {
    include!(concat!(env!("OUT_DIR"), "/glean_metrics.rs"));
}

uniffi::custom_type!(Guid, String, {
    remote,
    try_lift: |val| Ok(Guid::new(val.as_str())),
    lower: |obj| obj.into(),
});

uniffi::custom_type!(Url, String, {
    remote,
    try_lift: |val| {
        match Url::parse(val.as_str()) {
            Ok(url) => Ok(url),
            Err(e) => Err(PlacesApiError::UrlParseFailed {
                reason: e.to_string(),
            }
            .into()),
        }
    },
    lower: |obj| obj.into(),
});

uniffi::custom_type!(PlacesTimestamp, i64, {
    remote,
    try_lift: |val| Ok(PlacesTimestamp(val as u64)),
    lower: |obj| obj.as_millis() as i64,
});

uniffi::custom_type!(VisitTransitionSet, i32, {
    try_lift: |val| {
        Ok(VisitTransitionSet::from_u16(val as u16).expect("Bug: Invalid VisitTransitionSet"))
    },
    lower: |obj| VisitTransitionSet::into_u16(obj) as i32,
});

uniffi::include_scaffolding!("places");
