/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::*;
use rusqlite::Connection;

use std::sync::Mutex;

pub struct RemergeDb {
    db: Connection,
    info: super::bootstrap::RemergeInfo,
}

lazy_static::lazy_static! {
    static ref DB_INIT_MUTEX: Mutex<()> = Mutex::new(());
}

impl RemergeDb {
    pub fn with_connection(
        mut db: Connection,
        native: super::NativeSchemaInfo<'_>,
    ) -> Result<Self> {
        let pragmas = "
            -- The value we use was taken from Desktop Firefox, and seems necessary to
            -- help ensure good performance. The default value is 1024, which the SQLite
            -- docs themselves say is too small and should be changed.
            PRAGMA page_size = 32768;

            -- Disable calling mlock/munlock for every malloc/free.
            -- In practice this results in a massive speedup, especially
            -- for insert-heavy workloads.
            PRAGMA cipher_memory_security = false;

            -- `temp_store = 2` is required on Android to force the DB to keep temp
            -- files in memory, since on Android there's no tmp partition. See
            -- https://github.com/mozilla/mentat/issues/505. Ideally we'd only
            -- do this on Android, and/or allow caller to configure it.
            -- (although see also bug 1313021, where Firefox enabled it for both
            -- Android and 64bit desktop builds)
            PRAGMA temp_store = 2;

            -- We want foreign-key support.
            PRAGMA foreign_keys = ON;

            -- we unconditionally want write-ahead-logging mode
            PRAGMA journal_mode=WAL;

            -- How often to autocheckpoint (in units of pages).
            -- 2048000 (our max desired WAL size) / 32768 (page size).
            PRAGMA wal_autocheckpoint=62
        ";
        db.execute_batch(pragmas)?;
        let tx = db.transaction()?;
        super::schema::init(&tx)?;
        let info = super::bootstrap::load_or_bootstrap(&tx, native)?;
        tx.commit()?;
        Ok(RemergeDb { db, info })
    }
}
