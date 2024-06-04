/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use crate::{
    interest::InterestVectorKind,
    schema::RelevancyConnectionInitializer,
    url_hash::{hash_url, UrlHash},
    Interest, InterestVector, Result,
};
use interrupt_support::SqlInterruptScope;
use rusqlite::{Connection, OpenFlags};
use sql_support::{ConnExt, LazyDb};
use std::path::Path;

/// A thread-safe wrapper around an SQLite connection to the Relevancy database
pub struct RelevancyDb {
    reader: LazyDb<RelevancyConnectionInitializer>,
    writer: LazyDb<RelevancyConnectionInitializer>,
}

impl RelevancyDb {
    pub fn new(path: impl AsRef<Path>) -> Self {
        // Note: use `SQLITE_OPEN_READ_WRITE` for both read and write connections.
        // Even if we're opening a read connection, we may need to do a write as part of the
        // initialization process.
        //
        // The read-only nature of the connection is enforced by the fact that [RelevancyDb::read] uses a
        // shared ref to the `RelevancyDao`.
        let db_open_flags = OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_READ_WRITE;
        Self {
            reader: LazyDb::new(path.as_ref(), db_open_flags, RelevancyConnectionInitializer),
            writer: LazyDb::new(path.as_ref(), db_open_flags, RelevancyConnectionInitializer),
        }
    }

    pub fn close(&self) {
        self.reader.close(true);
        self.writer.close(true);
    }

    pub fn interrupt(&self) {
        self.reader.interrupt();
        self.writer.interrupt();
    }

    #[cfg(test)]
    pub fn new_for_test() -> Self {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let count = COUNTER.fetch_add(1, Ordering::Relaxed);
        Self::new(format!("file:test{count}.sqlite?mode=memory&cache=shared"))
    }

    /// Accesses the Suggest database in a transaction for reading.
    pub fn read<T>(&self, op: impl FnOnce(&RelevancyDao) -> Result<T>) -> Result<T> {
        let (mut conn, scope) = self.reader.lock()?;
        let tx = conn.transaction()?;
        let dao = RelevancyDao::new(&tx, scope);
        op(&dao)
    }

    /// Accesses the Suggest database in a transaction for reading and writing.
    pub fn read_write<T>(&self, op: impl FnOnce(&mut RelevancyDao) -> Result<T>) -> Result<T> {
        let (mut conn, scope) = self.writer.lock()?;
        let tx = conn.transaction()?;
        let mut dao = RelevancyDao::new(&tx, scope);
        let result = op(&mut dao)?;
        tx.commit()?;
        Ok(result)
    }
}

/// A data access object (DAO) that wraps a connection to the Relevancy database
///
/// Methods that only read from the database take an immutable reference to
/// `self` (`&self`), and methods that write to the database take a mutable
/// reference (`&mut self`).
pub struct RelevancyDao<'a> {
    pub conn: &'a Connection,
    pub scope: SqlInterruptScope,
}

impl<'a> RelevancyDao<'a> {
    fn new(conn: &'a Connection, scope: SqlInterruptScope) -> Self {
        Self { conn, scope }
    }

    /// Return Err(Interrupted) if we were interrupted
    pub fn err_if_interrupted(&self) -> Result<()> {
        Ok(self.scope.err_if_interrupted()?)
    }

    /// Associate a URL with an interest
    pub fn add_url_interest(&mut self, url_hash: UrlHash, interest: Interest) -> Result<()> {
        let sql = "
            INSERT OR REPLACE INTO url_interest(url_hash, interest_code)
            VALUES (?, ?)
        ";
        self.conn.execute(sql, (url_hash, interest as u32))?;
        Ok(())
    }

    /// Get an interest vector for a URL
    pub fn get_url_interest_vector(&self, url: &str) -> Result<InterestVector> {
        let hash = match hash_url(url) {
            Some(u) => u,
            None => return Ok(InterestVector::default()),
        };
        let mut stmt = self.conn.prepare_cached(
            "
            SELECT interest_code
            FROM url_interest
            WHERE url_hash=?
        ",
        )?;
        let interests = stmt.query_and_then((hash,), |row| -> Result<Interest> {
            row.get::<_, u32>(0)?.try_into()
        })?;

        let mut interest_vec = InterestVector::default();
        for interest in interests {
            interest_vec[interest?] += 1
        }
        Ok(interest_vec)
    }

    /// Do we need to load the interest data?
    pub fn need_to_load_url_interests(&self) -> Result<bool> {
        // TODO: we probably will need a better check than this.
        Ok(self
            .conn
            .query_one("SELECT NOT EXISTS (SELECT 1 FROM url_interest)")?)
    }

    /// Update the frecency user interest vector based on a new measurement.
    ///
    /// Right now this completely replaces the interest vector with the new data.  At some point,
    /// we may switch to incrementally updating it instead.
    pub fn update_frecency_user_interest_vector(&self, interests: &InterestVector) -> Result<()> {
        let mut stmt = self.conn.prepare(
            "
            INSERT OR REPLACE INTO user_interest(kind, interest_code, count)
            VALUES (?, ?, ?)
            ",
        )?;
        for (interest, count) in interests.as_vec() {
            stmt.execute((InterestVectorKind::Frecency, interest, count))?;
        }

        Ok(())
    }

    pub fn get_frecency_user_interest_vector(&self) -> Result<InterestVector> {
        let mut stmt = self
            .conn
            .prepare("SELECT interest_code, count FROM user_interest WHERE kind = ?")?;
        let mut interest_vec = InterestVector::default();
        let rows = stmt.query_and_then((InterestVectorKind::Frecency,), |row| {
            crate::Result::Ok((
                Interest::try_from(row.get::<_, u32>(0)?)?,
                row.get::<_, u32>(1)?,
            ))
        })?;
        for row in rows {
            let (interest_code, count) = row?;
            interest_vec.set(interest_code, count);
        }
        Ok(interest_vec)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_store_frecency_user_interest_vector() {
        let db = RelevancyDb::new_for_test();
        // Initially the interest vector should be blank
        assert_eq!(
            db.read_write(|dao| dao.get_frecency_user_interest_vector())
                .unwrap(),
            InterestVector::default()
        );

        let interest_vec = InterestVector {
            animals: 2,
            autos: 1,
            news: 5,
            ..InterestVector::default()
        };
        db.read_write(|dao| dao.update_frecency_user_interest_vector(&interest_vec))
            .unwrap();
        assert_eq!(
            db.read_write(|dao| dao.get_frecency_user_interest_vector())
                .unwrap(),
            interest_vec,
        );
    }

    #[test]
    fn test_update_frecency_user_interest_vector() {
        let db = RelevancyDb::new_for_test();
        let interest_vec1 = InterestVector {
            animals: 2,
            autos: 1,
            news: 5,
            ..InterestVector::default()
        };
        let interest_vec2 = InterestVector {
            animals: 1,
            career: 3,
            ..InterestVector::default()
        };
        // Update the first interest vec, then the second one
        db.read_write(|dao| dao.update_frecency_user_interest_vector(&interest_vec1))
            .unwrap();
        db.read_write(|dao| dao.update_frecency_user_interest_vector(&interest_vec2))
            .unwrap();
        // The current behavior is the second one should replace the first
        assert_eq!(
            db.read_write(|dao| dao.get_frecency_user_interest_vector())
                .unwrap(),
            interest_vec2,
        );
    }
}
