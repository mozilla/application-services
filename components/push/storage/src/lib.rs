/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 *
 * Handle Push data storage
 */
extern crate crypto;
#[cfg(test)]
extern crate hex;

mod db;
mod error;
mod record;
mod schema;
mod types;

pub use crate::{
    db::{PushDb as Store, Storage},
    record::PushRecord,
};
