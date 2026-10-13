/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! A SQLite-backed [`MetadataStore`], persisting sync bookkeeping across
//! restarts.
//!
//! Everything lives in one key/value `meta` table. The collection name is part
//! of the primary key, so many engines can share a database file. Each store is
//! scoped to one collection and only touches its own rows.

use crate::baseline::BaselineState;
use crate::error::{Error, Result};
use crate::event::{EventPosition, GateKey};
use crate::metadata::MetadataStore;
use crate::schema::SchemaVersion;
use rusqlite::{
    named_params,
    types::{FromSql, ToSql},
    Connection, OptionalExtension, Transaction,
};
use serde::{Deserialize, Serialize};
use sql_support::open_database::{
    self, open_database_with_flags, read_write_flags, ConnectionInitializer,
};
use std::path::Path;
use std::sync::Mutex;
use sync15::engine::{CollSyncIds, EngineSyncAssociation};
use sync15::{Guid, ServerTimestamp};

// SQLite and migration errors both surface as `Error::Metadata`.
impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::Metadata(e.to_string())
    }
}
impl From<open_database::Error> for Error {
    fn from(e: open_database::Error) -> Self {
        Error::Metadata(e.to_string())
    }
}

/// Schema version; bump it and add an `upgrade_from` arm on any schema change.
const VERSION: u32 = 1;

const SQL: &str = "
    CREATE TABLE meta (
        collection TEXT NOT NULL,
        key        TEXT NOT NULL,
        value      NOT NULL,
        PRIMARY KEY (collection, key)
    ) WITHOUT ROWID;
";

// `meta` keys within a collection.
const LAST_SYNC_KEY: &str = "last_sync";
const GLOBAL_SYNC_ID_KEY: &str = "global_sync_id";
const COLL_SYNC_ID_KEY: &str = "coll_sync_id";
const BASELINE_STATE_KEY: &str = "baseline_state";
/// Prefix for gate rows; the rest of the key is the JSON `GateKey`, so it's
/// unambiguous whatever the event type is.
const POSITION_PREFIX: &str = "position:";

struct MetadataConnectionInitializer;

impl ConnectionInitializer for MetadataConnectionInitializer {
    const NAME: &'static str = "event_based_sync_engine metadata";
    const END_VERSION: u32 = VERSION;

    fn prepare(&self, conn: &Connection, _db_empty: bool) -> open_database::Result<()> {
        sql_support::setup_sqlite_defaults(conn)?;
        Ok(())
    }

    fn init(&self, db: &Transaction<'_>) -> open_database::Result<()> {
        db.execute_batch(SQL)?;
        Ok(())
    }

    fn upgrade_from(&self, _tx: &Transaction<'_>, version: u32) -> open_database::Result<()> {
        // No prior versions yet.
        Err(open_database::Error::IncompatibleVersion(version))
    }
}

/// JSON-serializable form of [`BaselineState`] (positions and timestamps as
/// integer millis).
#[derive(Default, Serialize, Deserialize)]
struct StoredBaselineState {
    floor: Option<EventPosition>,
    newest_event: Option<EventPosition>,
    events_since_baseline: u64,
    max_schema_since_baseline: SchemaVersion,
    bar_blocked_since_ms: Option<i64>,
}

impl From<&BaselineState> for StoredBaselineState {
    fn from(s: &BaselineState) -> Self {
        Self {
            floor: s.floor.clone(),
            newest_event: s.newest_event.clone(),
            events_since_baseline: s.events_since_baseline,
            max_schema_since_baseline: s.max_schema_since_baseline,
            bar_blocked_since_ms: s.bar_blocked_since.map(|t| t.as_millis()),
        }
    }
}

impl From<StoredBaselineState> for BaselineState {
    fn from(s: StoredBaselineState) -> Self {
        Self {
            floor: s.floor,
            newest_event: s.newest_event,
            events_since_baseline: s.events_since_baseline,
            max_schema_since_baseline: s.max_schema_since_baseline,
            bar_blocked_since: s.bar_blocked_since_ms.map(ServerTimestamp::from_millis),
        }
    }
}

/// A SQLite-backed [`MetadataStore`], scoped to a single collection.
///
/// The `Mutex` makes the store `Sync` (as the trait requires); metadata traffic
/// is low-volume, so one serialized connection is plenty.
pub struct SqliteMetadataStore {
    conn: Mutex<Connection>,
    collection: String,
}

impl SqliteMetadataStore {
    /// Open (creating if needed) the database at `path`, scoped to `collection`.
    pub fn open(path: impl AsRef<Path>, collection: impl Into<String>) -> Result<Self> {
        let conn =
            open_database_with_flags(path, read_write_flags(), &MetadataConnectionInitializer)?;
        Ok(Self {
            conn: Mutex::new(conn),
            collection: collection.into(),
        })
    }

    /// Wrap an already-open connection (schema must exist), scoped to
    /// `collection`. Lets several collections share one connection.
    pub fn from_connection(conn: Connection, collection: impl Into<String>) -> Self {
        Self {
            conn: Mutex::new(conn),
            collection: collection.into(),
        }
    }

    /// An in-memory store, for tests and examples.
    #[cfg(test)]
    pub fn new_in_memory(collection: impl Into<String>) -> Self {
        let conn = open_database::open_memory_database(&MetadataConnectionInitializer).unwrap();
        Self::from_connection(conn, collection)
    }

    // `meta` key/value helpers, scoped to `self.collection`.

    fn put_meta(&self, conn: &Connection, key: &str, value: &dyn ToSql) -> Result<()> {
        conn.execute(
            "REPLACE INTO meta (collection, key, value) VALUES (:collection, :key, :value)",
            named_params! { ":collection": self.collection, ":key": key, ":value": value },
        )?;
        Ok(())
    }

    fn get_meta<T: FromSql>(&self, conn: &Connection, key: &str) -> Result<Option<T>> {
        Ok(conn
            .query_row(
                "SELECT value FROM meta WHERE collection = :collection AND key = :key",
                named_params! { ":collection": self.collection, ":key": key },
                |row| row.get(0),
            )
            .optional()?)
    }

    fn delete_meta(&self, conn: &Connection, key: &str) -> Result<()> {
        conn.execute(
            "DELETE FROM meta WHERE collection = :collection AND key = :key",
            named_params! { ":collection": self.collection, ":key": key },
        )?;
        Ok(())
    }
}

/// `meta` key for a gate key's last-seen position.
fn position_key(gate: &GateKey) -> Result<String> {
    Ok(format!(
        "{}{}",
        POSITION_PREFIX,
        serde_json::to_string(gate)?
    ))
}

impl MetadataStore for SqliteMetadataStore {
    fn position(&self, key: &GateKey) -> Result<Option<EventPosition>> {
        let conn = self.conn.lock().unwrap();
        Ok(match self.get_meta::<String>(&conn, &position_key(key)?)? {
            Some(json) => Some(serde_json::from_str::<EventPosition>(&json)?),
            None => None,
        })
    }

    fn set_position(&self, key: GateKey, pos: EventPosition) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let value = serde_json::to_string(&pos)?;
        self.put_meta(&conn, &position_key(&key)?, &value)
    }

    fn prune_positions(&self, covers_up_to: &EventPosition) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        // Drop positions strictly older than `covers_up_to`.
        let stale: Vec<String> = {
            let mut stmt = conn.prepare(
                "SELECT key, value FROM meta
                 WHERE collection = :collection AND key LIKE :pattern",
            )?;
            let rows = stmt.query_and_then(
                named_params! {
                    ":collection": self.collection,
                    ":pattern": format!("{}%", POSITION_PREFIX),
                },
                |row| -> Result<(String, EventPosition)> {
                    let key: String = row.get(0)?;
                    let value: String = row.get(1)?;
                    Ok((key, serde_json::from_str::<EventPosition>(&value)?))
                },
            )?;
            rows.filter_map(|r| match r {
                Ok((key, pos)) if !pos.supersedes(covers_up_to) && pos != *covers_up_to => {
                    Some(Ok(key))
                }
                Ok(_) => None,
                Err(e) => Some(Err(e)),
            })
            .collect::<Result<_>>()?
        };

        let tx = conn.transaction()?;
        for key in stale {
            self.delete_meta(&tx, &key)?;
        }
        tx.commit()?;
        Ok(())
    }

    fn last_sync(&self) -> Result<ServerTimestamp> {
        let conn = self.conn.lock().unwrap();
        Ok(self
            .get_meta::<i64>(&conn, LAST_SYNC_KEY)?
            .map(ServerTimestamp::from_millis)
            .unwrap_or(ServerTimestamp::EPOCH))
    }

    fn set_last_sync(&self, ts: ServerTimestamp) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        self.put_meta(&conn, LAST_SYNC_KEY, &ts.as_millis())
    }

    fn sync_assoc(&self) -> Result<EngineSyncAssociation> {
        let conn = self.conn.lock().unwrap();
        let global = self.get_meta::<String>(&conn, GLOBAL_SYNC_ID_KEY)?;
        let coll = self.get_meta::<String>(&conn, COLL_SYNC_ID_KEY)?;
        Ok(match (global, coll) {
            (Some(global), Some(coll)) => EngineSyncAssociation::Connected(CollSyncIds {
                global: Guid::new(&global),
                coll: Guid::new(&coll),
            }),
            _ => EngineSyncAssociation::Disconnected,
        })
    }

    fn set_sync_assoc(&self, assoc: EngineSyncAssociation) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        match assoc {
            EngineSyncAssociation::Connected(ids) => {
                self.put_meta(&tx, GLOBAL_SYNC_ID_KEY, &ids.global.as_str())?;
                self.put_meta(&tx, COLL_SYNC_ID_KEY, &ids.coll.as_str())?;
            }
            EngineSyncAssociation::Disconnected => {
                self.delete_meta(&tx, GLOBAL_SYNC_ID_KEY)?;
                self.delete_meta(&tx, COLL_SYNC_ID_KEY)?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    fn baseline_state(&self) -> Result<BaselineState> {
        let conn = self.conn.lock().unwrap();
        Ok(match self.get_meta::<String>(&conn, BASELINE_STATE_KEY)? {
            Some(json) => serde_json::from_str::<StoredBaselineState>(&json)?.into(),
            None => BaselineState::default(),
        })
    }

    fn set_baseline_state(&self, state: BaselineState) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let json = serde_json::to_string(&StoredBaselineState::from(&state))?;
        self.put_meta(&conn, BASELINE_STATE_KEY, &json)
    }

    fn reset(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        // Only this collection's rows; a shared table may hold others.
        conn.execute(
            "DELETE FROM meta WHERE collection = :collection",
            named_params! { ":collection": self.collection },
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLL: &str = "containers";

    fn pos(millis: i64, id: &str) -> EventPosition {
        EventPosition::new(ServerTimestamp::from_millis(millis), Guid::new(id))
    }

    fn key(event_type: &str, entity: &str) -> GateKey {
        GateKey::new(event_type, Guid::new(entity))
    }

    #[test]
    fn positions_round_trip_and_overwrite() {
        let store = SqliteMetadataStore::new_in_memory(COLL);
        let k = key("modify-name", "containerX");
        assert_eq!(store.position(&k).unwrap(), None);

        store.set_position(k.clone(), pos(100, "e100")).unwrap();
        assert_eq!(store.position(&k).unwrap(), Some(pos(100, "e100")));

        // A second set for the same key overwrites.
        store.set_position(k.clone(), pos(200, "e200")).unwrap();
        assert_eq!(store.position(&k).unwrap(), Some(pos(200, "e200")));

        // A different key is independent.
        let other = key("modify-color", "containerX");
        assert_eq!(store.position(&other).unwrap(), None);
    }

    #[test]
    fn prune_drops_strictly_older_positions() {
        let store = SqliteMetadataStore::new_in_memory(COLL);
        store
            .set_position(key("t", "old"), pos(100, "e100"))
            .unwrap();
        store
            .set_position(key("t", "at"), pos(200, "e200"))
            .unwrap();
        store
            .set_position(key("t", "new"), pos(300, "e300"))
            .unwrap();

        store.prune_positions(&pos(200, "e200")).unwrap();

        // Strictly older is gone; at-the-floor and newer survive.
        assert_eq!(store.position(&key("t", "old")).unwrap(), None);
        assert_eq!(
            store.position(&key("t", "at")).unwrap(),
            Some(pos(200, "e200"))
        );
        assert_eq!(
            store.position(&key("t", "new")).unwrap(),
            Some(pos(300, "e300"))
        );
    }

    #[test]
    fn prune_breaks_ties_on_record_id() {
        let store = SqliteMetadataStore::new_in_memory(COLL);
        // Same millis; record id decides order.
        store.set_position(key("t", "a"), pos(200, "e1")).unwrap();
        store.set_position(key("t", "b"), pos(200, "e2")).unwrap();

        store.prune_positions(&pos(200, "e2")).unwrap();

        assert_eq!(store.position(&key("t", "a")).unwrap(), None);
        assert_eq!(
            store.position(&key("t", "b")).unwrap(),
            Some(pos(200, "e2"))
        );
    }

    #[test]
    fn last_sync_defaults_to_epoch() {
        let store = SqliteMetadataStore::new_in_memory(COLL);
        assert_eq!(store.last_sync().unwrap(), ServerTimestamp::EPOCH);

        let ts = ServerTimestamp::from_millis(12345);
        store.set_last_sync(ts).unwrap();
        assert_eq!(store.last_sync().unwrap(), ts);
    }

    #[test]
    fn sync_assoc_round_trips() {
        let store = SqliteMetadataStore::new_in_memory(COLL);
        assert_eq!(
            store.sync_assoc().unwrap(),
            EngineSyncAssociation::Disconnected
        );

        let ids = CollSyncIds {
            global: Guid::new("g"),
            coll: Guid::new("c"),
        };
        store
            .set_sync_assoc(EngineSyncAssociation::Connected(ids.clone()))
            .unwrap();
        assert_eq!(
            store.sync_assoc().unwrap(),
            EngineSyncAssociation::Connected(ids)
        );

        // Disconnecting clears it.
        store
            .set_sync_assoc(EngineSyncAssociation::Disconnected)
            .unwrap();
        assert_eq!(
            store.sync_assoc().unwrap(),
            EngineSyncAssociation::Disconnected
        );
    }

    #[test]
    fn baseline_state_round_trips() {
        let store = SqliteMetadataStore::new_in_memory(COLL);
        assert_eq!(store.baseline_state().unwrap(), BaselineState::default());

        let state = BaselineState {
            floor: Some(pos(100, "floor")),
            newest_event: Some(pos(300, "newest")),
            events_since_baseline: 7,
            max_schema_since_baseline: 3,
            bar_blocked_since: Some(ServerTimestamp::from_millis(500)),
        };
        store.set_baseline_state(state.clone()).unwrap();
        assert_eq!(store.baseline_state().unwrap(), state);
    }

    #[test]
    fn reset_clears_only_this_collection() {
        // Two collections sharing one file-backed `meta` table.
        let dir = TempDir::new();
        let path = dir.path().join("shared.db");

        let a = SqliteMetadataStore::open(&path, "collA").unwrap();
        let b = SqliteMetadataStore::open(&path, "collB").unwrap();

        a.set_position(key("t", "x"), pos(100, "e100")).unwrap();
        a.set_last_sync(ServerTimestamp::from_millis(1)).unwrap();
        b.set_position(key("t", "y"), pos(200, "e200")).unwrap();
        b.set_last_sync(ServerTimestamp::from_millis(2)).unwrap();

        a.reset().unwrap();

        // A is wiped, B is untouched.
        assert_eq!(a.position(&key("t", "x")).unwrap(), None);
        assert_eq!(a.last_sync().unwrap(), ServerTimestamp::EPOCH);
        assert_eq!(b.position(&key("t", "y")).unwrap(), Some(pos(200, "e200")));
        assert_eq!(b.last_sync().unwrap(), ServerTimestamp::from_millis(2));
    }

    #[test]
    fn collections_are_isolated_in_one_table() {
        let dir = TempDir::new();
        let path = dir.path().join("shared.db");
        let a = SqliteMetadataStore::open(&path, "collA").unwrap();
        let b = SqliteMetadataStore::open(&path, "collB").unwrap();

        // Same gate key in both collections stays distinct.
        a.set_position(key("t", "x"), pos(100, "eA")).unwrap();
        b.set_position(key("t", "x"), pos(200, "eB")).unwrap();
        assert_eq!(a.position(&key("t", "x")).unwrap(), Some(pos(100, "eA")));
        assert_eq!(b.position(&key("t", "x")).unwrap(), Some(pos(200, "eB")));

        // Everything landed in the single `meta` table.
        let count: i64 = a
            .conn
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM meta", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn persists_across_reopen() {
        let dir = TempDir::new();
        let path = dir.path().join("meta.db");

        {
            let store = SqliteMetadataStore::open(&path, COLL).unwrap();
            store.set_position(key("t", "x"), pos(100, "e100")).unwrap();
            store
                .set_last_sync(ServerTimestamp::from_millis(42))
                .unwrap();
        }

        let store = SqliteMetadataStore::open(&path, COLL).unwrap();
        assert_eq!(
            store.position(&key("t", "x")).unwrap(),
            Some(pos(100, "e100"))
        );
        assert_eq!(store.last_sync().unwrap(), ServerTimestamp::from_millis(42));
    }

    /// Minimal temp-dir helper so the file-backed tests don't need a new dependency.
    struct TempDir {
        path: std::path::PathBuf,
    }
    impl TempDir {
        fn new() -> Self {
            let mut path = std::env::temp_dir();
            path.push(format!(
                "event_based_sync_engine_meta_test_{}",
                Guid::random().as_str()
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self { path }
        }
        fn path(&self) -> &Path {
            &self.path
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}
