/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

mod coop_transaction;

use crate::api::places_api::ConnectionType;
use crate::error::*;
use coop_transaction::ChunkedCoopTransaction;
use rusqlite::Connection;
use sql_support::{ConnExt, UncheckedTransaction};

/// High level transaction type which "does the right thing" for you.
/// Construct one with `PlacesDb::begin_transaction()`.
pub enum PlacesTransaction<'conn> {
    Sync(ChunkedCoopTransaction<'conn>),
    Write(UncheckedTransaction<'conn>),
    Read(UncheckedTransaction<'conn>),
}

impl<'conn> PlacesTransaction<'conn> {
    /// - For transactions on sync connnections: Checks to see if we have held a
    ///   transaction for longer than the requested time, and if so, commits the
    ///   current transaction and opens another.
    /// - For transactions on other connections: `debug_assert!`s, or logs a
    ///   warning and does nothing.
    #[inline]
    pub fn maybe_commit(&mut self) -> Result<()> {
        if let PlacesTransaction::Sync(tx) = self {
            tx.maybe_commit()?;
        } else if cfg!(debug_assertions) {
            panic!(
                "maybe_commit called on non-sync transaction (this is a no-op in release build)"
            );
        } else {
            // Log an error even though it's extremely non-fatal. Maybe we'll
            // see it.
            log::error!("maybe_commit called on a non-sync transaction");
        }
        Ok(())
    }

    /// Consumes and commits a PlacesTransaction transaction.
    pub fn commit(self) -> Result<()> {
        match self {
            PlacesTransaction::Sync(t) => t.commit()?,
            PlacesTransaction::Write(t) => t.commit()?,
            PlacesTransaction::Read(_) => {
                log::warn!("Commit on a read transaction does nothing");
            }
        };
        Ok(())
    }

    /// Consumes and attempst to roll back a PlacesTransaction. Note that if
    /// maybe_commit has been called, this may only roll back as far as that
    /// call.
    pub fn rollback(self) -> Result<()> {
        match self {
            PlacesTransaction::Sync(t) => t.rollback()?,
            PlacesTransaction::Write(t) => t.rollback()?,
            PlacesTransaction::Read(_) => {
                log::warn!("Rollback on a read transaction does nothing");
            }
        };
        Ok(())
    }
}

impl super::PlacesDb {
    /// Begin the "correct" transaction type for this connection.
    pub fn begin_transaction(&self) -> Result<PlacesTransaction> {
        Ok(match self.conn_type() {
            ConnectionType::Sync => PlacesTransaction::Sync(self.chunked_coop_trransaction()?),
            ConnectionType::ReadWrite => PlacesTransaction::Write(self.coop_transaction()?),
            ConnectionType::ReadOnly => PlacesTransaction::Read(self.unchecked_transaction()?),
        })
    }
}

impl<'conn> std::ops::Deref for PlacesTransaction<'conn> {
    type Target = Connection;

    #[inline]
    fn deref(&self) -> &Connection {
        match self {
            PlacesTransaction::Sync(t) => &t,
            PlacesTransaction::Write(t) => &t,
            PlacesTransaction::Read(t) => &t,
        }
    }
}

impl<'conn> ConnExt for PlacesTransaction<'conn> {
    #[inline]
    fn conn(&self) -> &Connection {
        &*self
    }
}
