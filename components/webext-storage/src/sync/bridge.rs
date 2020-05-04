/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use sync15_traits::{self, ApplyResults, IncomingEnvelope};

use crate::api;
use crate::db::StorageDb;
use crate::error::{Error, ErrorKind, Result};

/// A bridged engine implements all the methods needed to make the
/// `storage.sync` store work with Desktop's Sync implementation.
/// Conceptually, it's similar to `sync15_traits::Store`, which we
/// should eventually rename and unify with this trait (#2841).
pub struct BridgedEngine<'a> {
    db: &'a StorageDb,
}

impl<'a> BridgedEngine<'a> {
    /// Creates a bridged engine for syncing.
    pub fn new(db: &'a StorageDb) -> Self {
        BridgedEngine { db }
    }
}

impl<'a> sync15_traits::BridgedEngine for BridgedEngine<'a> {
    type Error = Error;

    fn last_sync(&self) -> Result<i64> {
        Err(ErrorKind::NotImplemented.into())
    }

    fn set_last_sync(&self, _last_sync_millis: i64) -> Result<()> {
        Err(ErrorKind::NotImplemented.into())
    }

    fn sync_id(&self) -> Result<Option<String>> {
        Err(ErrorKind::NotImplemented.into())
    }

    fn reset_sync_id(&self) -> Result<String> {
        Err(ErrorKind::NotImplemented.into())
    }

    fn ensure_current_sync_id(&self, _new_sync_id: &str) -> Result<String> {
        Err(ErrorKind::NotImplemented.into())
    }

    fn store_incoming(&self, _incoming_envelopes: &[IncomingEnvelope]) -> Result<()> {
        Err(ErrorKind::NotImplemented.into())
    }

    fn apply(&self) -> Result<ApplyResults> {
        Err(ErrorKind::NotImplemented.into())
    }

    fn set_uploaded(&self, _server_modified_millis: i64, _ids: &[String]) -> Result<()> {
        Err(ErrorKind::NotImplemented.into())
    }

    fn sync_finished(&self) -> Result<()> {
        Err(ErrorKind::NotImplemented.into())
    }

    fn reset(&self) -> Result<()> {
        Err(ErrorKind::NotImplemented.into())
    }

    fn wipe(&self) -> Result<()> {
        let tx = self.db.unchecked_transaction()?;
        api::wipe_all(&tx)?;
        tx.commit()?;
        Ok(())
    }
}
