/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

mod android;
mod cargo_metadata;
mod ios;
mod toml;

pub use android::generate_android;
pub use ios::{generate_ios, generate_ios_focus};
