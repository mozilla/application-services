/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

mod engine;
pub(crate) mod merge;
mod payload;
mod update_plan;

pub use engine::LoginsSyncEngine;
use payload::{IncomingLogin, LoginPayload};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[repr(u8)]
pub(crate) enum SyncStatus {
    Synced = 0,
    Changed = 1,
    New = 2,
}
