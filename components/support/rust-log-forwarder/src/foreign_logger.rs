/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Logger interface for foreign code
//!
//! This is what the application code defines.  It's responsible for taking rust log records and
//! feeding them to the application logging system.

pub use log::Level;

/// log::Record, except it exposes it's data as fields rather than methods
#[derive(Debug, PartialEq, Eq)]
pub struct Record {
    pub level: Level,
    pub target: String,
    pub message: String,
}

pub trait Logger: Sync + Send {
    fn log(&self, record: Record);
}

impl From<&log::Record<'_>> for Record {
    fn from(record: &log::Record) -> Self {
        Self {
            level: record.level(),
            target: record.target().to_string(),
            message: record.args().to_string(),
        }
    }
}
