/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use camino::Utf8PathBuf;

use crate::{Attachment, RemoteSettingsRecord, Result};

/// Internal storage type
///
/// This will store downloaded records/attachments in a SQLite database.  Nothing is implemented
/// yet other than the initial API.
///
/// Most methods input a `collection_url` parameter, is a URL that includes the remote settings
/// server, bucket, and collection. If the `collection_url` for a get method does not match the one
/// for a set method, then this means the application has switched their remote settings config and
/// [Storage] should pretend like nothing is stored in the database.
///
/// The reason for this is the [crate::RemoteSettingsService::update_config] method.  If a consumer
/// passes a new server or bucket to `update_config`, we don't want to be using cached data from
/// the previous config.
///
/// Notes:
///   - I'm thinking we'll create a separate SQLite database per collection.  That reduces
///     contention when multiple clients try to get records at once.
///   - Still, there might be contention if there are multiple clients for the same collection, or
///     if RemoteSettingsService::sync() and RemoteSettingsClient::get_records(true) are called at
///     the same time.  Maybe we should create a single write connection and put it behind a mutex
///     to avoid the possibility of SQLITE_BUSY.  Or maybe not, the writes seem like they should be
///     very fast.
///   - Maybe we should refactor this to use the DAO pattern like suggest does.
pub struct Storage {}

impl Storage {
    pub fn new(_path: Utf8PathBuf) -> Result<Self> {
        Ok(Self {})
    }

    /// Get the last modified timestamp for the stored records
    ///
    /// Returns None if no records are stored or if `collection_url` does not match the
    /// `collection_url` passed to `set_records`.
    pub fn get_last_modified_timestamp(&self, _collection_url: &str) -> Result<Option<u64>> {
        Ok(None)
    }

    /// Get cached records for this collection
    ///
    /// Returns None if no records are stored or if `collection_url` does not match the
    /// `collection_url` passed to `set_records`.
    pub fn get_records(&self, _collection_url: &str) -> Result<Option<Vec<RemoteSettingsRecord>>> {
        Ok(None)
    }

    /// Get cached attachment data
    ///
    /// This returns the last attachment data sent to [Self::set_attachment].
    ///
    /// Returns None if no attachment data is stored or if `collection_url` does not match the `collection_url`
    /// passed to `set_attachment`.
    pub fn get_attachment(
        &self,
        _collection_url: &str,
        _attachment_id: &str,
    ) -> Result<Option<Attachment>> {
        Ok(None)
    }

    /// Set the list of records stored in the database, clearing out any previously stored records
    pub fn set_records(
        &self,
        _collection_url: &str,
        records: &[RemoteSettingsRecord],
    ) -> Result<()> {
        for record in records {
            println!("Should store record: {record:?}");
        }
        Ok(())
    }

    /// Set the attachment data stored in the database, clearing out any previously stored data
    pub fn set_attachment(
        &self,
        _collection_url: &str,
        attachment_id: &str,
        _attachment: Attachment,
    ) -> Result<()> {
        println!("Should store attachment: {attachment_id}");
        Ok(())
    }

    /// Empty out all cached values and start from scratch.  This is called when
    /// RemoteSettingsService::update_config() is called, since that could change the remote
    /// settings server which would invalidate all cached data.
    pub fn empty(&self) -> Result<()> {
        Ok(())
    }
}
