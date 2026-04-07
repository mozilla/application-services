/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::api::{self, BreachAlertDismissal};
use crate::db::{BreachAlertsDb, ThreadSafeBreachAlertsDb};
use crate::error::*;
use error_support::handle_error;
use std::path::Path;
use std::sync::Arc;

use interrupt_support::SqlInterruptHandle;

/// A store for managing breach alert data. It manages an underlying
/// database connection and exposes methods for reading and writing
/// breach alert dismissals.
///
/// An application should create only one store, and manage the instance
/// as a singleton.
#[derive(uniffi::Object)]
pub struct BreachAlertsStore {
    pub(crate) db: Arc<ThreadSafeBreachAlertsDb>,
}

impl BreachAlertsStore {
    pub fn new(db_path: impl AsRef<Path>) -> Result<Self> {
        let db = BreachAlertsDb::new(db_path)?;
        Ok(Self {
            db: Arc::new(ThreadSafeBreachAlertsDb::new(db)),
        })
    }

    #[cfg(test)]
    pub fn new_memory(db_path: &str) -> Result<Self> {
        let db = BreachAlertsDb::new_memory(db_path)?;
        Ok(Self {
            db: Arc::new(ThreadSafeBreachAlertsDb::new(db)),
        })
    }

    pub fn interrupt_handle(&self) -> Arc<SqlInterruptHandle> {
        self.db.interrupt_handle()
    }
}

#[uniffi::export]
impl BreachAlertsStore {
    #[uniffi::constructor]
    pub fn new_store(db_path: String) -> ApiResult<Self> {
        Self::new(db_path).map_err(|e| BreachAlertsApiError::Unexpected {
            reason: e.to_string(),
        })
    }

    #[handle_error(Error)]
    pub fn get_breach_alert_dismissals(
        &self,
        breach_names: Vec<String>,
    ) -> ApiResult<Vec<BreachAlertDismissal>> {
        let db = self.db.lock();
        let conn = db.get_connection()?;
        api::get_breach_alert_dismissals(conn, &breach_names)
    }

    #[handle_error(Error)]
    pub fn set_breach_alert_dismissals(
        &self,
        dismissals: Vec<BreachAlertDismissal>,
    ) -> ApiResult<()> {
        let db = self.db.lock();
        let conn = db.get_connection()?;
        let tx = conn.unchecked_transaction()?;
        api::set_breach_alert_dismissals(&tx, &dismissals)?;
        tx.commit()?;
        Ok(())
    }

    #[handle_error(Error)]
    pub fn clear_breach_alert_dismissals(&self, breach_names: Vec<String>) -> ApiResult<()> {
        let db = self.db.lock();
        let conn = db.get_connection()?;
        let tx = conn.unchecked_transaction()?;
        api::clear_breach_alert_dismissals(&tx, &breach_names)?;
        tx.commit()?;
        Ok(())
    }

    #[handle_error(Error)]
    pub fn clear_all_breach_alert_dismissals(&self) -> ApiResult<()> {
        let db = self.db.lock();
        let conn = db.get_connection()?;
        let tx = conn.unchecked_transaction()?;
        api::clear_all_breach_alert_dismissals(&tx)?;
        tx.commit()?;
        Ok(())
    }

    #[handle_error(Error)]
    pub fn close(&self) -> ApiResult<()> {
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
