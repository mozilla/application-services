/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use rusqlite::{
    self,
    types::{FromSql, ToSql},
    Connection, Result as SqlResult, Row, Savepoint, Transaction, TransactionBehavior, NO_PARAMS,
};
use std::iter::FromIterator;
use std::ops::Deref;
use std::time::{Duration, Instant};

use crate::maybe_cached::MaybeCached;

pub struct Conn(rusqlite::Connection);

/// This trait exists so that we can use these helpers on `rusqlite::{Transaction, Connection}`.
/// Note that you must import ConnExt in order to call these methods on anything.
pub trait ConnExt {
    /// The method you need to implement to opt in to all of this.
    fn conn(&self) -> &Connection;

    /// Get a cached or uncached statement based on a flag.
    fn prepare_maybe_cached<'conn>(
        &'conn self,
        sql: &str,
        cache: bool,
    ) -> SqlResult<MaybeCached<'conn>> {
        MaybeCached::prepare(self.conn(), sql, cache)
    }

    /// Execute all the provided statements.
    fn execute_all(&self, stmts: &[&str]) -> SqlResult<()> {
        let conn = self.conn();
        for sql in stmts {
            conn.execute(sql, NO_PARAMS)?;
        }
        Ok(())
    }

    /// Equivalent to `Connection::execute_named` but caches the statement so that subsequent
    /// calls to `execute_cached` will have improved performance.
    fn execute_cached<P>(&self, sql: &str, params: P) -> SqlResult<usize>
    where
        P: IntoIterator,
        P::Item: ToSql,
    {
        let mut stmt = self.conn().prepare_cached(sql)?;
        stmt.execute(params)
    }

    /// Equivalent to `Connection::execute_named` but caches the statement so that subsequent
    /// calls to `execute_named_cached` will have imprroved performance.
    fn execute_named_cached(&self, sql: &str, params: &[(&str, &dyn ToSql)]) -> SqlResult<usize> {
        crate::maybe_log_plan(self.conn(), sql, params);
        let mut stmt = self.conn().prepare_cached(sql)?;
        stmt.execute_named(params)
    }

    /// Execute a query that returns a single result column, and return that result.
    fn query_one<T: FromSql>(&self, sql: &str) -> SqlResult<T> {
        crate::maybe_log_plan(self.conn(), sql, &[]);
        let res: T = self
            .conn()
            .query_row_and_then(sql, NO_PARAMS, |row| row.get_checked(0))?;
        Ok(res)
    }

    /// Execute a query that returns 0 or 1 result columns, returning None
    /// if there were no rows, or if the only result was NULL.
    fn try_query_one<T: FromSql>(
        &self,
        sql: &str,
        params: &[(&str, &ToSql)],
        cache: bool,
    ) -> SqlResult<Option<T>>
    where
        Self: Sized,
    {
        crate::maybe_log_plan(self.conn(), sql, params);
        use rusqlite::OptionalExtension;
        // The outer option is if we got rows, the inner option is
        // if the first row was null.
        let res: Option<Option<T>> = self
            .conn()
            .query_row_and_then_named(sql, params, |row| row.get_checked(0), cache)
            .optional()?;
        // go from Option<Option<T>> to Option<T>
        Ok(res.unwrap_or_default())
    }

    /// Equivalent to `rusqlite::Connection::query_row_and_then` but allows use
    /// of named parameters, and allows passing a flag to indicate that it's cached.
    fn query_row_and_then_named<T, E, F>(
        &self,
        sql: &str,
        params: &[(&str, &dyn ToSql)],
        mapper: F,
        cache: bool,
    ) -> Result<T, E>
    where
        Self: Sized,
        E: From<rusqlite::Error>,
        F: FnOnce(&Row) -> Result<T, E>,
    {
        crate::maybe_log_plan(self.conn(), sql, params);
        Ok(self
            .try_query_row(sql, params, mapper, cache)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)?)
    }

    /// Helper for when you'd like to get a Vec<T> of all the rows returned by a
    /// query that takes named arguments. See also
    /// `query_rows_and_then_named_cached`.
    fn query_rows_and_then_named<T, E, F>(
        &self,
        sql: &str,
        params: &[(&str, &dyn ToSql)],
        mapper: F,
    ) -> Result<Vec<T>, E>
    where
        Self: Sized,
        E: From<rusqlite::Error>,
        F: FnMut(&Row) -> Result<T, E>,
    {
        crate::maybe_log_plan(self.conn(), sql, params);
        query_rows_and_then_named(self.conn(), sql, params, mapper, false)
    }

    /// Helper for when you'd like to get a Vec<T> of all the rows returned by a
    /// query that takes named arguments.
    fn query_rows_and_then_named_cached<T, E, F>(
        &self,
        sql: &str,
        params: &[(&str, &dyn ToSql)],
        mapper: F,
    ) -> Result<Vec<T>, E>
    where
        Self: Sized,
        E: From<rusqlite::Error>,
        F: FnMut(&Row) -> Result<T, E>,
    {
        crate::maybe_log_plan(self.conn(), sql, params);
        query_rows_and_then_named(self.conn(), sql, params, mapper, false)
    }

    /// Like `query_rows_and_then_named`, but works if you want a non-Vec as a result.
    /// # Example:
    /// ```rust,no_run
    /// # use std::collections::HashSet;
    /// # use sql_support::ConnExt;
    /// # use rusqlite::Connection;
    /// fn get_visit_tombstones(conn: &Connection, id: i64) -> rusqlite::Result<HashSet<i64>> {
    ///     Ok(conn.query_rows_into(
    ///         "SELECT visit_date FROM moz_historyvisit_tombstones
    ///          WHERE place_id = :place_id",
    ///         &[(":place_id", &id)],
    ///         |row| row.get_checked::<_, i64>(0))?)
    /// }
    /// ```
    /// Note if the type isn't inferred, you'll have to do something gross like
    /// `conn.query_rows_into::<HashSet<_>, _, _, _>(...)`.
    fn query_rows_into<Coll, T, E, F>(
        &self,
        sql: &str,
        params: &[(&str, &dyn ToSql)],
        mapper: F,
    ) -> Result<Coll, E>
    where
        Self: Sized,
        E: From<rusqlite::Error>,
        F: FnMut(&Row) -> Result<T, E>,
        Coll: FromIterator<T>,
    {
        crate::maybe_log_plan(self.conn(), sql, params);
        query_rows_and_then_named(self.conn(), sql, params, mapper, false)
    }

    /// Same as `query_rows_into`, but caches the stmt if possible.
    fn query_rows_into_cached<Coll, T, E, F>(
        &self,
        sql: &str,
        params: &[(&str, &dyn ToSql)],
        mapper: F,
    ) -> Result<Coll, E>
    where
        Self: Sized,
        E: From<rusqlite::Error>,
        F: FnMut(&Row) -> Result<T, E>,
        Coll: FromIterator<T>,
    {
        crate::maybe_log_plan(self.conn(), sql, params);
        query_rows_and_then_named(self.conn(), sql, params, mapper, true)
    }

    // This should probably have a longer name...
    /// Like `query_row_and_then_named` but returns None instead of erroring if no such row exists.
    fn try_query_row<T, E, F>(
        &self,
        sql: &str,
        params: &[(&str, &dyn ToSql)],
        mapper: F,
        cache: bool,
    ) -> Result<Option<T>, E>
    where
        Self: Sized,
        E: From<rusqlite::Error>,
        F: FnOnce(&Row) -> Result<T, E>,
    {
        crate::maybe_log_plan(self.conn(), sql, params);
        let conn = self.conn();
        let mut stmt = MaybeCached::prepare(conn, sql, cache)?;
        let mut rows = stmt.query_named(params)?;
        Ok(match rows.next() {
            None => None,
            Some(row_res) => {
                let row = row_res?;
                Some(mapper(&row)?)
            }
        })
    }

    fn unchecked_transaction(&self) -> SqlResult<UncheckedTransaction> {
        UncheckedTransaction::new(self.conn(), TransactionBehavior::Deferred)
    }

    fn time_chunked_transaction(
        &self,
        commit_after: Duration,
    ) -> SqlResult<TimeChunkedTransaction> {
        TimeChunkedTransaction::new(self.conn(), TransactionBehavior::Deferred, commit_after)
    }
}

impl ConnExt for Connection {
    #[inline]
    fn conn(&self) -> &Connection {
        self
    }
}

impl<'conn> ConnExt for Transaction<'conn> {
    #[inline]
    fn conn(&self) -> &Connection {
        &*self
    }
}

impl<'conn> ConnExt for Savepoint<'conn> {
    #[inline]
    fn conn(&self) -> &Connection {
        &*self
    }
}

/// rusqlite, in an attempt to save us from ourselves, needs a mutable ref to
/// a connection to start a transaction. That is a bit of a PITA in some cases,
/// so we offer this as an alternative - but the responsibility of ensuring
/// there are no concurrent transactions is on our head.
///
/// This is very similar to the rusqlite `Transaction` - it doesn't prevent
/// against nested transactions but does allow you to use an immutable
/// `Connection`.
pub struct UncheckedTransaction<'conn> {
    conn: &'conn Connection,
    started_at: Instant,
    finished: bool,
    // we could add drop_behavior etc too, but we don't need it yet - we
    // always rollback.
}

impl<'conn> UncheckedTransaction<'conn> {
    /// Begin a new unchecked transaction. Cannot be nested, but this is not
    /// enforced by Rust (hence 'unchecked') - however, it is enforced by
    /// SQLite; use a rusqlite `savepoint` for nested transactions.
    pub fn new(conn: &'conn Connection, behavior: TransactionBehavior) -> SqlResult<Self> {
        let query = match behavior {
            TransactionBehavior::Deferred => "BEGIN DEFERRED",
            TransactionBehavior::Immediate => "BEGIN IMMEDIATE",
            TransactionBehavior::Exclusive => "BEGIN EXCLUSIVE",
        };
        conn.execute_batch(query)
            .map(move |_| UncheckedTransaction {
                conn,
                started_at: Instant::now(),
                finished: false,
            })
    }

    /// Consumes and commits an unchecked transaction.
    pub fn commit(mut self) -> SqlResult<()> {
        if self.finished {
            log::warn!("ignoring request to commit an already finished transaction");
            return Ok(());
        }
        self.finished = true;
        self.conn.execute_batch("COMMIT")?;
        log::debug!("Transaction commited after {:?}", self.started_at.elapsed());
        Ok(())
    }

    /// Consumes and rolls back an unchecked transaction.
    pub fn rollback(mut self) -> SqlResult<()> {
        if self.finished {
            log::warn!("ignoring request to rollback an already finished transaction");
            return Ok(());
        }
        self.rollback_()
    }

    fn rollback_(&mut self) -> SqlResult<()> {
        self.finished = true;
        self.conn.execute_batch("ROLLBACK")?;
        Ok(())
    }

    fn finish_(&mut self) -> SqlResult<()> {
        if self.finished || self.conn.is_autocommit() {
            return Ok(());
        }
        self.rollback_()?;
        Ok(())
    }
}

impl<'conn> Deref for UncheckedTransaction<'conn> {
    type Target = Connection;

    #[inline]
    fn deref(&self) -> &Connection {
        self.conn
    }
}

impl<'conn> Drop for UncheckedTransaction<'conn> {
    fn drop(&mut self) {
        if let Err(e) = self.finish_() {
            log::warn!("Error dropping an unchecked transaction: {}", e);
        }
    }
}

impl<'conn> ConnExt for UncheckedTransaction<'conn> {
    #[inline]
    fn conn(&self) -> &Connection {
        &*self
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
    ) -> SqlResult<Self> {
        Ok(Self {
            tx: UncheckedTransaction::new(conn, behavior)?,
            behavior,
            commit_after,
        })
    }

    /// Checks to see if we have held a transaction for longer than the
    /// requested time, and if so, commits the current transaction and opens
    /// another.
    #[inline]
    pub fn maybe_commit(&mut self) -> SqlResult<()> {
        if self.tx.started_at.elapsed() >= self.commit_after {
            log::debug!("TimeChunkedTransaction commiting after taking allocated time");
            self.commit_and_start_new_tx()?;
        }
        Ok(())
    }

    #[inline(never)]
    fn commit_and_start_new_tx(&mut self) -> SqlResult<()> {
        // We can't call self.tx.commit() here as it wants to consume
        // self.tx, and we can't set up the new self.tx first as then
        // we'll be trying to start a new transaction while the current
        // one is in progress. So explicitly set the finished flag on it.
        self.tx.finished = true;
        self.tx.execute_batch("COMMIT")?;
        self.tx = UncheckedTransaction::new(self.tx.conn, self.behavior)?;
        Ok(())
    }

    /// Consumes and commits a TimeChunkedTransaction transaction.
    pub fn commit(self) -> SqlResult<()> {
        self.tx.commit()
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

fn query_rows_and_then_named<Coll, T, E, F>(
    conn: &Connection,
    sql: &str,
    params: &[(&str, &dyn ToSql)],
    mapper: F,
    cache: bool,
) -> Result<Coll, E>
where
    E: From<rusqlite::Error>,
    F: FnMut(&Row) -> Result<T, E>,
    Coll: FromIterator<T>,
{
    let mut stmt = conn.prepare_maybe_cached(sql, cache)?;
    let iter = stmt.query_and_then_named(params, mapper)?;
    Ok(iter.collect::<Result<Coll, E>>()?)
}
