/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::env;

// This includes the generated Glean Rust bindings, which get exposed
// via the `mod glean_metrics` include in lib.rs
include!(concat!(env!("OUT_DIR"), "/glean_metrics.rs"));
