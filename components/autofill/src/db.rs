/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use crate::error::*;
use crate::schema;

use rusqlite::{Connection, OpenFlags};
use std::{
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
};
use url::Url;

use crate::api::{addresses, credit_cards};
use sync_guid::Guid;

pub struct AutofillDb {
    pub writer: Connection,
}

impl AutofillDb {
    pub fn new(db_path: impl AsRef<Path>) -> Result<Self> {
        let db_path = normalize_path(db_path)?;
        Self::new_named(db_path)
    }

    #[allow(dead_code)]
    pub fn add_credit_card(
        &self,
        new_credit_card_fields: credit_cards::NewCreditCardFields,
    ) -> Result<credit_cards::CreditCard> {
        credit_cards::add_credit_card(&self.writer, new_credit_card_fields)
    }

    #[allow(dead_code)]
    pub fn get_credit_card(&self, guid: &Guid) -> Result<credit_cards::CreditCard> {
        credit_cards::get_credit_card(&self.writer, guid)
    }

    #[allow(dead_code)]
    pub fn get_all_credit_cards(&self) -> Result<Vec<credit_cards::CreditCard>> {
        credit_cards::get_all_credit_cards(&self.writer)
    }

    #[allow(dead_code)]
    pub fn update_credit_card(&self, credit_card: credit_cards::CreditCard) -> Result<()> {
        credit_cards::update_credit_card(&self.writer, credit_card)
    }

    #[allow(dead_code)]
    pub fn delete_credit_card(&self, guid: &Guid) -> Result<bool> {
        credit_cards::delete_credit_card(&self.writer, guid)
    }

    #[allow(dead_code)]
    pub fn add_address(
        &self,
        new_address: addresses::NewAddressFields,
    ) -> Result<addresses::Address> {
        addresses::add_address(&self.writer, new_address)
    }

    #[allow(dead_code)]
    pub fn get_address(&self, guid: &Guid) -> Result<addresses::Address> {
        addresses::get_address(&self.writer, guid)
    }

    #[allow(dead_code)]
    pub fn get_all_addresses(&self) -> Result<Vec<addresses::Address>> {
        addresses::get_all_addresses(&self.writer)
    }

    #[allow(dead_code)]
    pub fn update_address(&self, address: addresses::Address) -> Result<()> {
        addresses::update_address(&self.writer, address)
    }

    #[allow(dead_code)]
    pub fn delete_address(&self, guid: &Guid) -> Result<bool> {
        addresses::delete_address(&self.writer, guid)
    }

    #[cfg(test)]
    pub fn new_memory(db_path: &str) -> Result<Self> {
        let name = PathBuf::from(format!("file:{}?mode=memory&cache=shared", db_path));
        Self::new_named(name)
    }

    #[allow(dead_code)]
    fn new_named(db_path: PathBuf) -> Result<Self> {
        // We always create the read-write connection for an initial open so
        // we can create the schema and/or do version upgrades.
        let flags = OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_READ_WRITE;

        let conn = Connection::open_with_flags(db_path, flags)?;

        #[allow(dead_code)]
        init_sql_connection(&conn, true)?;
        Ok(Self { writer: conn })
    }
}

impl Deref for AutofillDb {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        &self.writer
    }
}

impl DerefMut for AutofillDb {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.writer
    }
}

fn init_sql_connection(conn: &Connection, is_writable: bool) -> Result<()> {
    define_functions(&conn)?;
    conn.set_prepared_statement_cache_capacity(128);
    if is_writable {
        let tx = conn.unchecked_transaction()?;
        schema::init(&conn)?;
        tx.commit()?;
    };
    Ok(())
}

fn unurl_path(p: impl AsRef<Path>) -> PathBuf {
    p.as_ref()
        .to_str()
        .and_then(|s| Url::parse(s).ok())
        .and_then(|u| {
            if u.scheme() == "file" {
                u.to_file_path().ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| p.as_ref().to_owned())
}

fn normalize_path(p: impl AsRef<Path>) -> Result<PathBuf> {
    let path = unurl_path(p);
    if let Ok(canonical) = path.canonicalize() {
        return Ok(canonical);
    }
    // It probably doesn't exist yet. This is an error, although it seems to
    // work on some systems.
    //
    // We resolve this by trying to canonicalize the parent directory, and
    // appending the requested file name onto that. If we can't canonicalize
    // the parent, we return an error.
    //
    // Also, we return errors if the path ends in "..", if there is no
    // parent directory, etc.
    let file_name = path
        .file_name()
        .ok_or_else(|| ErrorKind::IllegalDatabasePath(path.clone()))?;

    let parent = path
        .parent()
        .ok_or_else(|| ErrorKind::IllegalDatabasePath(path.clone()))?;

    let mut canonical = parent.canonicalize()?;
    canonical.push(file_name);
    Ok(canonical)
}

#[allow(dead_code)]
fn define_functions(c: &Connection) -> Result<()> {
    use rusqlite::functions::FunctionFlags;
    c.create_scalar_function(
        "generate_guid",
        0,
        FunctionFlags::SQLITE_UTF8,
        sql_fns::generate_guid,
    )?;
    Ok(())
}

pub(crate) mod sql_fns {
    use rusqlite::{functions::Context, Result};
    use sync_guid::Guid as SyncGuid;

    #[inline(never)]
    #[allow(dead_code)]
    pub fn generate_guid(_ctx: &Context<'_>) -> Result<SyncGuid> {
        Ok(SyncGuid::random())
    }
}

// Helpers for tests
#[cfg(test)]
pub mod test {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // A helper for our tests to get their own memory Api.
    static ATOMIC_COUNTER: AtomicUsize = AtomicUsize::new(0);

    pub fn new_mem_db() -> AutofillDb {
        let _ = env_logger::try_init();
        let counter = ATOMIC_COUNTER.fetch_add(1, Ordering::Relaxed);
        AutofillDb::new_memory(&format!("test_autofill-api-{}", counter))
            .expect("should get an API")
    }
}
