/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub mod bootstrap;
mod bundle;
pub mod db;
pub(crate) mod meta;
pub mod records;
pub mod schema;
pub(crate) mod upgrades;

pub use bundle::SchemaBundle;
pub use db::RemergeDb;
pub use records::{LocalRecord, NativeRecord, RawRecord};

// This doesn't really belong here.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[repr(u8)]
pub enum SyncStatus {
    Synced = 0,
    Changed = 1,
    New = 2,
}
