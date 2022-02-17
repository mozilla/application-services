/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub mod error;
pub mod intermediate_representation;
pub mod parser;
pub mod backends;
mod util;

#[cfg(test)]
#[allow(dead_code)]
pub mod fixtures;
