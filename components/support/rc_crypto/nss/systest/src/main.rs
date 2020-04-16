/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(bad_style)]
#![cfg_attr(feature = "cargo-clippy", allow(clippy::all))]
use nss_sys::*;

include!(concat!(env!("OUT_DIR"), "/all.rs"));
