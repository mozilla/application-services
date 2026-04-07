/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::db::{BreachAlertsDb, ThreadSafeBreachAlertsDb};
use crate::error::*;
use std::path::Path;
use std::sync::Arc;

use interrupt_support::SqlInterruptHandle;

/// A store for managing breach alert data. It manages an underlying
/// database connection and exposes methods for reading and writing
/// breach alert dismissals.
///
/// An application should create only one store, and manage the instance
/// as a singleton.
pub struct BreachAlertsStore {
    pub(crate) db: Arc<ThreadSafeBreachAlertsDb>,
}

impl BreachAlertsStore {
    /// Creates a store backed by a database at `db_path`. The path can be a
    /// file path or `file:` URI.
    pub fn new(db_path: impl AsRef<Path>) -> Result<Self> {
        let db = BreachAlertsDb::new(db_path)?;
        Ok(Self {
            db: Arc::new(ThreadSafeBreachAlertsDb::new(db)),
        })
    }

    /// Creates a store backed by an in-memory database.
    #[cfg(test)]
    pub fn new_memory(db_path: &str) -> Result<Self> {
        let db = BreachAlertsDb::new_memory(db_path)?;
        Ok(Self {
            db: Arc::new(ThreadSafeBreachAlertsDb::new(db)),
        })
    }

    /// Returns an interrupt handle for this store.
    pub fn interrupt_handle(&self) -> Arc<SqlInterruptHandle> {
        self.db.interrupt_handle()
    }

    /// Closes the store and its database connection.
    pub fn close(&self) -> Result<()> {
        let mut db = self.db.lock();
        db.close()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_send() {
        fn ensure_send<T: Send>() {}
        ensure_send::<BreachAlertsStore>();
    }
}
