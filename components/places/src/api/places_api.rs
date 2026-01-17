/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::bookmark_sync::BookmarksSyncEngine;
use crate::db::db::{PlacesDb, SharedPlacesDb};
use crate::error::*;
use crate::history_sync::HistorySyncEngine;
use crate::util::normalize_path;
use error_support::handle_error;
use interrupt_support::register_interrupt;
use lazy_static::lazy_static;
use parking_lot::Mutex;
use rusqlite::OpenFlags;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Weak,
};
use sync15::engine::{SyncEngine, SyncEngineId};

// Our "sync manager" will use whatever is stashed here.
lazy_static::lazy_static! {
    // Mutex: just taken long enough to update the contents - needed to wrap
    //        the Weak as it isn't `Sync`
    // [Arc/Weak]: Stores the places api used to create the connection for
    //             BookmarksSyncEngine/HistorySyncEngine
    static ref PLACES_API_FOR_SYNC_MANAGER: Mutex<Weak<PlacesApi>> = Mutex::new(Weak::new());
}

// Called by the sync manager to get a sync engine via the PlacesApi previously
// registered with the sync manager.
pub fn get_registered_sync_engine(engine_id: &SyncEngineId) -> Option<Box<dyn SyncEngine>> {
    match PLACES_API_FOR_SYNC_MANAGER.lock().upgrade() {
        None => {
            warn!("places: get_registered_sync_engine: no PlacesApi registered");
            None
        }
        Some(places_api) => match create_sync_engine(&places_api, engine_id) {
            Ok(engine) => Some(engine),
            Err(e) => {
                // Report this to Sentry, except if it's an open database error.  That indicates
                // that there is a registered sync engine, but the connection is busy so we can't
                // open it.  This is a known issue that we don't need more reports for (see
                // https://github.com/mozilla/application-services/issues/5237 for discussion).
                if !matches!(e, Error::OpenDatabaseError(_)) {
                    error_support::report_error!(
                        "places-no-registered-sync-engine",
                        "places: get_registered_sync_engine: {}",
                        e
                    );
                }
                None
            }
        },
    }
}

fn create_sync_engine(
    places_api: &PlacesApi,
    engine_id: &SyncEngineId,
) -> Result<Box<dyn SyncEngine>> {
    let conn = places_api.get_sync_connection()?;
    match engine_id {
        SyncEngineId::Bookmarks => Ok(Box::new(BookmarksSyncEngine::new(conn)?)),
        SyncEngineId::History => Ok(Box::new(HistorySyncEngine::new(conn)?)),
        _ => unreachable!("can't provide unknown engine: {}", engine_id),
    }
}

#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ConnectionType {
    ReadOnly = 1,
    ReadWrite = 2,
    Sync = 3,
}

impl ConnectionType {
    pub fn from_primitive(p: u8) -> Option<Self> {
        match p {
            1 => Some(ConnectionType::ReadOnly),
            2 => Some(ConnectionType::ReadWrite),
            3 => Some(ConnectionType::Sync),
            _ => None,
        }
    }
}

impl ConnectionType {
    pub fn rusqlite_flags(self) -> OpenFlags {
        let common_flags = OpenFlags::SQLITE_OPEN_NO_MUTEX | OpenFlags::SQLITE_OPEN_URI;
        match self {
            ConnectionType::ReadOnly => common_flags | OpenFlags::SQLITE_OPEN_READ_ONLY,
            ConnectionType::ReadWrite => {
                common_flags | OpenFlags::SQLITE_OPEN_CREATE | OpenFlags::SQLITE_OPEN_READ_WRITE
            }
            ConnectionType::Sync => common_flags | OpenFlags::SQLITE_OPEN_READ_WRITE,
        }
    }
}

// We only allow a single PlacesApi per filename.
lazy_static! {
    static ref APIS: Mutex<HashMap<PathBuf, Weak<PlacesApi>>> = Mutex::new(HashMap::new());
}

static ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// For uniffi we need to expose our `Arc` returning constructor as a global function :(
/// https://github.com/mozilla/uniffi-rs/pull/1063 would fix this, but got some pushback
/// meaning we are forced into this unfortunate workaround.
#[handle_error(crate::Error)]
pub fn places_api_new(db_name: impl AsRef<Path>) -> ApiResult<Arc<PlacesApi>> {
    PlacesApi::new(db_name)
}

/// The entry-point to the places API. This object gives access to database
/// connections and other helpers. It enforces that only 1 write connection
/// can exist to the database at once.
pub struct PlacesApi {
    db_name: PathBuf,
    write_connection: Mutex<Option<PlacesDb>>,
    coop_tx_lock: Arc<Mutex<()>>,
    // Used for get_sync_connection()
    // - The inner mutex synchronizes sync operation (for example one of the [SyncEngine] methods).
    //   This avoids issues like #867
    // - The weak facilitates connection sharing.  When `get_sync_connection()` returns an Arc, we
    //   keep a weak reference to it.  If the Arc is still alive when `get_sync_connection()` is
    //   called again, we reuse it.
    // - The outer mutex synchronizes the `get_sync_connection()` operation.  If multiple threads
    //   ran that at the same time there would be issues.
    sync_connection: Mutex<Weak<SharedPlacesDb>>,
    id: usize,
}

impl PlacesApi {
    /// Create a new, or fetch an already open, PlacesApi backed by a file on disk.
    pub fn new(db_name: impl AsRef<Path>) -> Result<Arc<Self>> {
        let db_name = normalize_path(db_name)?;
        Self::new_or_existing(db_name)
    }

    /// Create a new, or fetch an already open, memory-based PlacesApi. You must
    /// provide a name, but you are still able to have a single writer and many
    ///  reader connections to the same memory DB open.
    pub fn new_memory(db_name: &str) -> Result<Arc<Self>> {
        let name = PathBuf::from(format!("file:{}?mode=memory&cache=shared", db_name));
        Self::new_or_existing(name)
    }
    fn new_or_existing_into(
        target: &mut HashMap<PathBuf, Weak<PlacesApi>>,
        db_name: PathBuf,
    ) -> Result<Arc<Self>> {
        let id = ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        match target.get(&db_name).and_then(Weak::upgrade) {
            Some(existing) => Ok(existing),
            None => {
                // We always create a new read-write connection for an initial open so
                // we can create the schema and/or do version upgrades.
                let coop_tx_lock = Arc::new(Mutex::new(()));
                let connection = PlacesDb::open(
                    &db_name,
                    ConnectionType::ReadWrite,
                    id,
                    coop_tx_lock.clone(),
                )?;
                let new = PlacesApi {
                    db_name: db_name.clone(),
                    write_connection: Mutex::new(Some(connection)),
                    sync_connection: Mutex::new(Weak::new()),
                    id,
                    coop_tx_lock,
                };
                let arc = Arc::new(new);
                target.insert(db_name, Arc::downgrade(&arc));
                Ok(arc)
            }
        }
    }

    fn new_or_existing(db_name: PathBuf) -> Result<Arc<Self>> {
        let mut guard = APIS.lock();
        Self::new_or_existing_into(&mut guard, db_name)
    }

    /// Open a connection to the database.
    pub fn open_connection(&self, conn_type: ConnectionType) -> Result<PlacesDb> {
        match conn_type {
            ConnectionType::ReadOnly => {
                // make a new one - we can have as many of these as we want.
                PlacesDb::open(
                    self.db_name.clone(),
                    ConnectionType::ReadOnly,
                    self.id,
                    self.coop_tx_lock.clone(),
                )
            }
            ConnectionType::ReadWrite => {
                // We only allow one of these.
                let mut guard = self.write_connection.lock();
                match guard.take() {
                    None => Err(Error::ConnectionAlreadyOpen),
                    Some(db) => Ok(db),
                }
            }
            ConnectionType::Sync => {
                panic!("Use `get_sync_connection` to open a sync connection");
            }
        }
    }

    // Get a database connection to sync with
    //
    // This function provides a couple features to facilitate sharing the connection between
    // different sync engines:
    //   - Each connection is wrapped in a `Mutex<>` to synchronize access.
    //   - The mutex is then wrapped in an Arc<>.  If the last Arc<> returned is still alive, then
    //     get_sync_connection() will reuse it.
    pub fn get_sync_connection(&self) -> Result<Arc<SharedPlacesDb>> {
        // First step: lock the outer mutex
        let mut conn = self.sync_connection.lock();
        match conn.upgrade() {
            // If our Weak is still alive, then re-use that
            Some(db) => Ok(db),
            // If not, create a new connection
            None => {
                let db = Arc::new(SharedPlacesDb::new(PlacesDb::open(
                    self.db_name.clone(),
                    ConnectionType::Sync,
                    self.id,
                    self.coop_tx_lock.clone(),
                )?));
                register_interrupt(Arc::<SharedPlacesDb>::downgrade(&db));
                // Store a weakref for next time
                *conn = Arc::downgrade(&db);
                Ok(db)
            }
        }
    }

    /// Close a connection to the database. If the connection is the write
    /// connection, you can re-fetch it using open_connection.
    pub fn close_connection(&self, connection: PlacesDb) -> Result<()> {
        if connection.api_id() != self.id {
            return Err(Error::WrongApiForClose);
        }
        if connection.conn_type() == ConnectionType::ReadWrite {
            // We only allow one of these.
            let mut guard = self.write_connection.lock();
            assert!((*guard).is_none());
            *guard = Some(connection);
        }
        Ok(())
    }

    // This allows the embedding app to say "make this instance available to
    // the sync manager". The implementation is more like "offer to sync mgr"
    // (thereby avoiding us needing to link with the sync manager) but
    // `register_with_sync_manager()` is logically what's happening so that's
    // the name it gets.
    pub fn register_with_sync_manager(self: Arc<Self>) {
        *PLACES_API_FOR_SYNC_MANAGER.lock() = Arc::downgrade(&self);
    }
}

#[cfg(test)]
pub mod test {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // A helper for our tests to get their own memory Api.
    static ATOMIC_COUNTER: AtomicUsize = AtomicUsize::new(0);

    pub fn new_mem_api() -> Arc<PlacesApi> {
        // A bit hacky, but because this is a test-only function that almost all tests use,
        // it's a convenient place to initialize logging for tests.
        error_support::init_for_tests();

        let counter = ATOMIC_COUNTER.fetch_add(1, Ordering::Relaxed);
        PlacesApi::new_memory(&format!("test-api-{}", counter)).expect("should get an API")
    }

    pub fn new_mem_connection() -> PlacesDb {
        new_mem_api()
            .open_connection(ConnectionType::ReadWrite)
            .expect("should get a connection")
    }

    pub struct MemConnections {
        pub read: PlacesDb,
        pub write: PlacesDb,
        pub api: Arc<PlacesApi>,
    }

    pub fn new_mem_connections() -> MemConnections {
        let api = new_mem_api();
        let read = api
            .open_connection(ConnectionType::ReadOnly)
            .expect("should get a read connection");
        let write = api
            .open_connection(ConnectionType::ReadWrite)
            .expect("should get a write connection");
        MemConnections { read, write, api }
    }
}

#[cfg(test)]
mod tests {
    use super::test::*;
    use super::*;
    use sql_support::ConnExt;

    #[test]
    fn test_multi_writers_fails() {
        let api = new_mem_api();
        let writer1 = api
            .open_connection(ConnectionType::ReadWrite)
            .expect("should get writer");
        api.open_connection(ConnectionType::ReadWrite)
            .expect_err("should fail to get second writer");
        // But we should be able to re-get it after closing it.
        api.close_connection(writer1)
            .expect("should be able to close");
        api.open_connection(ConnectionType::ReadWrite)
            .expect("should get a writer after closing the other");
    }

    #[test]
    fn test_shared_memory() {
        let api = new_mem_api();
        let writer = api
            .open_connection(ConnectionType::ReadWrite)
            .expect("should get writer");
        writer
            .execute_batch(
                "CREATE TABLE test_table (test_value INTEGER);
                              INSERT INTO test_table VALUES (999)",
            )
            .expect("should insert");
        let reader = api
            .open_connection(ConnectionType::ReadOnly)
            .expect("should get reader");
        let val = reader
            .conn_ext_query_one::<i64>("SELECT test_value FROM test_table")
            .expect("should get value");
        assert_eq!(val, 999);
    }

    #[test]
    fn test_reader_before_writer() {
        let api = new_mem_api();
        let reader = api
            .open_connection(ConnectionType::ReadOnly)
            .expect("should get reader");
        let writer = api
            .open_connection(ConnectionType::ReadWrite)
            .expect("should get writer");
        writer
            .execute_batch(
                "CREATE TABLE test_table (test_value INTEGER);
                              INSERT INTO test_table VALUES (999)",
            )
            .expect("should insert");
        let val = reader
            .conn_ext_query_one::<i64>("SELECT test_value FROM test_table")
            .expect("should get value");
        assert_eq!(val, 999);
    }

    #[test]
    fn test_wrong_writer_close() {
        let api = new_mem_api();
        // Grab this so `api` doesn't think it still has a writer.
        let _writer = api
            .open_connection(ConnectionType::ReadWrite)
            .expect("should get writer");

        let fake_api = new_mem_api();
        let fake_writer = fake_api
            .open_connection(ConnectionType::ReadWrite)
            .expect("should get writer 2");

        assert!(matches!(
            api.close_connection(fake_writer).unwrap_err(),
            Error::WrongApiForClose
        ));
    }

    #[test]
    fn test_valid_writer_close() {
        let api = new_mem_api();
        let writer = api
            .open_connection(ConnectionType::ReadWrite)
            .expect("should get writer");

        api.close_connection(writer)
            .expect("Should allow closing own connection");

        // Make sure we can open it again.
        assert!(api.open_connection(ConnectionType::ReadWrite).is_ok());
    }
}
