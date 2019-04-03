/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::db::PlacesDb;
use crate::error::*;
use rusqlite::{Connection, TransactionBehavior};
use sql_support::{ConnExt, UncheckedTransaction};
use std::ops::Deref;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// This implements "cooperative transactions" for places. It relies on our
/// decision to have exactly 1 general purpose "writer" connection and exactly
/// one "sync writer" - ie, exactly 2 write connections.
///
/// The idea is that anything that uses the sync connection should use
/// `time_chunked_transaction`. Code using this should regularly call
/// `maybe_commit()`, and every second, will commit the transaction and start
/// a new one.
///
/// This means that in theory the other writable connection can start
/// transactions and should block for a max of 1 second - well under the 5
/// seconds before that other writer will fail with a SQLITE_BUSY or similar error.
///
/// However, in practice we see the writer thread being starved - even though
/// it's waiting for a lock, the sync thread still manages to re-get the lock.
/// In other words, the locks used by sqlite aren't "fair".
///
/// So we mitigate this with a simple approach that works fine in these
/// constraints.
/// * Each database connection shares a mutex.
/// * Before starting a transaction, each connection locks the mutex.
/// * It then starts an "exclusive" transaction - because sqlite now holds a
///   lock on our behalf, we release the lock on the mutex.
///
/// The end result here is that if either connection is waiting for the
/// database lock because the other already holds it, the waiting one is
/// guaranteed to get the database lock next.
impl PlacesDb {
    pub fn time_chunked_transaction(
        &self,
        commit_after: Duration,
    ) -> Result<TimeChunkedTransaction> {
        Ok(TimeChunkedTransaction::new(
            self.conn(),
            TransactionBehavior::Exclusive,
            commit_after,
            &self.coop_tx_lock,
        )?)
    }

    pub fn unchecked_transaction(&self) -> Result<UncheckedTransaction> {
        let _lock = self.coop_tx_lock.lock().unwrap();
        Ok(UncheckedTransaction::new(
            self.conn(),
            TransactionBehavior::Exclusive,
        )?)
    }
}

/// This transaction is suitable for when a transaction is used purely for
/// performance reasons rather than for data-integrity reasons, or when it's
/// used for integrity but held longer than strictly necessary for performance
/// reasons (ie, when it could be multiple transactions and still guarantee
/// integrity.) Examples of this might be for performance when updating a larger
/// number of rows, but data integrity concerns could be addressed by using
/// multiple, smaller transactions.
///
/// You can specify a duration for the maximum amount of time the transaction
/// should be held. You should regularly call .maybe_commit() as part of your
/// processing, and if the current transaction has been open for greater than
/// the specified duration the transaction will be committed and another one
/// started. You should always call .commit() at the end. Note that there is
/// no .rollback() method as it will be very difficult to work out what was
/// previously commited and therefore what was rolled back - if you need to
/// explicitly roll-back, this probably isn't what you should be using. Note
/// that SQLite might rollback for its own reasons though.
///
/// Note that this can still be used for transactions which ensure data
/// integrity. For example, if you are processing a large group of items, and
/// each individual item requires multiple updates, you will probably want to
/// ensure you don't call .maybe_commit() after every item rather than after
/// each individual database update.
pub struct TimeChunkedTransaction<'conn> {
    tx: UncheckedTransaction<'conn>,
    behavior: TransactionBehavior,
    commit_after: Duration,
    coop: &'conn Arc<Mutex<()>>,
}

impl<'conn> TimeChunkedTransaction<'conn> {
    /// Begin a new transaction which may be split into multiple transactions
    /// for performance reasons. Cannot be nested, but this is not
    /// enforced - however, it is enforced by SQLite; use a rusqlite `savepoint`
    /// for nested transactions.
    pub fn new(
        conn: &'conn Connection,
        behavior: TransactionBehavior,
        commit_after: Duration,
        coop: &'conn Arc<Mutex<()>>,
    ) -> Result<Self> {
        //        let _ = coop.lock().unwrap();
        Ok(Self {
            tx: UncheckedTransaction::new(conn, behavior)?,
            behavior,
            commit_after,
            coop,
        })
    }

    /// Checks to see if we have held a transaction for longer than the
    /// requested time, and if so, commits the current transaction and opens
    /// another.
    #[inline]
    pub fn maybe_commit(&mut self) -> Result<()> {
        if self.tx.started_at.elapsed() >= self.commit_after {
            log::debug!("TimeChunkedTransaction commiting after taking allocated time");
            self.commit_and_start_new_tx()?;
        }
        Ok(())
    }

    #[inline(never)]
    fn commit_and_start_new_tx(&mut self) -> Result<()> {
        // We can't call self.tx.commit() here as it wants to consume
        // self.tx, and we can't set up the new self.tx first as then
        // we'll be trying to start a new transaction while the current
        // one is in progress. So explicitly set the finished flag on it.
        self.tx.finished = true;
        self.tx.execute_batch("COMMIT")?;
        // acquire a lock on our cooperator - if our only other writer
        // thread holds a write lock we'll block until it is released.
        let _lock = self.coop.lock().unwrap();
        self.tx = UncheckedTransaction::new(self.tx.conn, self.behavior)?;
        Ok(())
    }

    /// Consumes and commits a TimeChunkedTransaction transaction.
    pub fn commit(self) -> Result<()> {
        Ok(self.tx.commit()?)
    }
}

impl<'conn> Deref for TimeChunkedTransaction<'conn> {
    type Target = Connection;

    #[inline]
    fn deref(&self) -> &Connection {
        self.tx.conn
    }
}

impl<'conn> ConnExt for TimeChunkedTransaction<'conn> {
    #[inline]
    fn conn(&self) -> &Connection {
        &*self
    }
}
