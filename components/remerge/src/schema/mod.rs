/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#[macro_use]
mod serde_utils;

pub mod desc;

pub mod error;
pub mod json;
pub mod merge_kinds;

pub use desc::*;
pub use error::{SchemaError, SchemaResult};
pub use json::{parse_from_string, RawSchema};
pub use merge_kinds::*;
