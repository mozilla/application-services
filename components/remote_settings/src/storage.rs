/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use camino::Utf8Path;

use crate::{RemoteSettingsRecord, Result};

/// Storage container
///
/// This uses SQLite to store records and attachments in the database.
pub struct Storage {}

// TODO: actually implement these methods

impl Storage {
    pub fn new(_db_path: &Utf8Path) -> Result<Self> {
        Ok(Self {})
    }

    /// Get all records currently in storage.
    ///
    /// Returns None if update_records has never been called.  Note: this is different from
    /// update_records being called with 0 items.
    pub fn get_records(&self) -> Result<Option<Vec<RemoteSettingsRecord>>> {
        Ok(None)
    }

    /// Update the stored records
    ///
    /// records is a list of remote settings records downloaded from the server.  If any have
    /// `deleted=true` then that record should be removed from storage.
    pub fn update_records(
        &self,
        _records: Vec<RemoteSettingsRecord>,
        _last_modified: u64,
    ) -> Result<()> {
        Ok(())
    }

    /// Get the last_modified time from the most recent call to [Self::update_records].
    pub fn last_modified_time(&self) -> Result<Option<u64>> {
        Ok(None)
    }

    /// Get attachment data from storage
    pub fn get_attachment_data(&self, _location: &str) -> Result<Option<Vec<u8>>> {
        // Q: should we store attachment data directly in SQLite or put it in the filesystem?
        todo!()
    }

    /// Store attachment data in storage
    pub fn store_attachment_data(&self, _location: &str, _data: &[u8]) -> Result<()> {
        todo!()
    }
}
