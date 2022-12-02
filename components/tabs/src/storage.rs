/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// From https://searchfox.org/mozilla-central/rev/ea63a0888d406fae720cf24f4727d87569a8cab5/services/sync/modules/constants.js#75
const URI_LENGTH_MAX: usize = 65536;
// https://searchfox.org/mozilla-central/rev/ea63a0888d406fae720cf24f4727d87569a8cab5/services/sync/modules/engines/tabs.js#8
const TAB_ENTRIES_LIMIT: usize = 5;

use crate::error::*;
use crate::schema;
use crate::sync::record::TabsRecord;
use crate::DeviceType;
use rusqlite::{
    types::{FromSql, ToSql},
    Connection, OpenFlags,
};
use serde_derive::{Deserialize, Serialize};
use sql_support::open_database::{self, open_database_with_flags};
use sql_support::ConnExt;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use sync15::RemoteClient;
use sync15::ServerTimestamp;

pub type TabsDeviceType = crate::DeviceType;
pub type RemoteTabRecord = RemoteTab;

pub(crate) const TTL_3_WEEKS: u32 = 15_552_000; // 21 days

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteTab {
    pub title: String,
    pub url_history: Vec<String>,
    pub icon: Option<String>,
    pub last_used: i64, // In ms.
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClientRemoteTabs {
    // The fxa_device_id of the client. *Should not* come from the id in the `clients` collection,
    // because that may or may not be the fxa_device_id (currently, it will not be for desktop
    // records.)
    pub client_id: String,
    pub client_name: String,
    #[serde(
        default = "devicetype_default_deser",
        skip_serializing_if = "devicetype_is_unknown"
    )]
    pub device_type: DeviceType,
    // serde default so we can read old rows that didn't persist this.
    #[serde(default)]
    pub last_modified: i64,
    pub remote_tabs: Vec<RemoteTab>,
}

fn devicetype_default_deser() -> DeviceType {
    // replace with `DeviceType::default_deser` once #4861 lands.
    DeviceType::Unknown
}

// Unlike most other uses-cases, here we do allow serializing ::Unknown, but skip it.
fn devicetype_is_unknown(val: &DeviceType) -> bool {
    matches!(val, DeviceType::Unknown)
}

// Tabs has unique requirements for storage:
// * The "local_tabs" exist only so we can sync them out. There's no facility to
//   query "local tabs", so there's no need to store these persistently - ie, they
//   are write-only.
// * The "remote_tabs" exist purely for incoming items via sync - there's no facility
//   to set them locally - they are read-only.
// Note that this means a database is only actually needed after Sync fetches remote tabs,
// and because sync users are in the minority, the use of a database here is purely
// optional and created on demand. The implication here is that asking for the "remote tabs"
// when no database exists is considered a normal situation and just implies no remote tabs exist.
// (Note however we don't attempt to remove the database when no remote tabs exist, so having
// no remote tabs in an existing DB is also a normal situation)
pub struct TabsStorage {
    local_tabs: RefCell<Option<Vec<RemoteTab>>>,
    db_path: PathBuf,
    db_connection: Option<Connection>,
}

impl TabsStorage {
    pub fn new(db_path: impl AsRef<Path>) -> Self {
        Self {
            local_tabs: RefCell::default(),
            db_path: db_path.as_ref().to_path_buf(),
            db_connection: None,
        }
    }

    /// Arrange for a new memory-based TabsStorage. As per other DB semantics, creating
    /// this isn't enough to actually create the db!
    pub fn new_with_mem_path(db_path: &str) -> Self {
        let name = PathBuf::from(format!("file:{}?mode=memory&cache=shared", db_path));
        Self::new(name)
    }

    /// If a DB file exists, open and return it.
    pub fn open_if_exists(&mut self) -> Result<Option<&Connection>> {
        if let Some(ref existing) = self.db_connection {
            return Ok(Some(existing));
        }
        let flags = OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_READ_WRITE;
        match open_database_with_flags(
            self.db_path.clone(),
            flags,
            &crate::schema::TabsMigrationLogic,
        ) {
            Ok(conn) => {
                self.db_connection = Some(conn);
                Ok(self.db_connection.as_ref())
            }
            Err(open_database::Error::SqlError(rusqlite::Error::SqliteFailure(code, _)))
                if code.code == rusqlite::ErrorCode::CannotOpen =>
            {
                Ok(None)
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Open and return the DB, creating it if necessary.
    pub fn open_or_create(&mut self) -> Result<&Connection> {
        if let Some(ref existing) = self.db_connection {
            return Ok(existing);
        }
        let flags = OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE;
        let conn = open_database_with_flags(
            self.db_path.clone(),
            flags,
            &crate::schema::TabsMigrationLogic,
        )?;
        self.db_connection = Some(conn);
        Ok(self.db_connection.as_ref().unwrap())
    }

    pub fn update_local_state(&mut self, local_state: Vec<RemoteTab>) {
        self.local_tabs.borrow_mut().replace(local_state);
    }

    pub fn prepare_local_tabs_for_upload(&self) -> Option<Vec<RemoteTab>> {
        if let Some(local_tabs) = self.local_tabs.borrow().as_ref() {
            return Some(
                local_tabs
                    .iter()
                    .cloned()
                    .filter_map(|mut tab| {
                        if tab.url_history.is_empty() || !is_url_syncable(&tab.url_history[0]) {
                            return None;
                        }
                        let mut sanitized_history = Vec::with_capacity(TAB_ENTRIES_LIMIT);
                        for url in tab.url_history {
                            if sanitized_history.len() == TAB_ENTRIES_LIMIT {
                                break;
                            }
                            if is_url_syncable(&url) {
                                sanitized_history.push(url);
                            }
                        }
                        tab.url_history = sanitized_history;
                        Some(tab)
                    })
                    .collect(),
            );
        }
        None
    }

    pub fn get_remote_tabs(&mut self) -> Option<Vec<ClientRemoteTabs>> {
        let remote_clients: HashMap<String, RemoteClient> =
            match self.get_meta::<String>(schema::REMOTE_CLIENTS_KEY).unwrap() {
                None => HashMap::default(),
                Some(json) => serde_json::from_str(&json).unwrap(),
            };
        match self.open_if_exists() {
            Err(e) => {
                error_support::report_error!(
                    "tabs-read-remote",
                    "Failed to read remote tabs: {}",
                    e
                );
                None
            }
            Ok(None) => None,
            Ok(Some(c)) => {
                let records: Option<Vec<(TabsRecord, ServerTimestamp)>> = match c
                    .query_rows_and_then_cached(
                        "SELECT record, last_modified FROM tabs",
                        [],
                        |row| -> Result<_> {
                            Ok((
                                serde_json::from_str(&row.get::<_, String>(0)?)?,
                                ServerTimestamp(row.get::<_, i64>(1)?),
                            ))
                        },
                    ) {
                    Ok(records) => Some(records),
                    Err(e) => {
                        error_support::report_error!(
                            "tabs-read-remote",
                            "Failed to read database: {}",
                            e
                        );
                        None
                    }
                };
                let mut crts: Vec<ClientRemoteTabs> = Vec::new();
                for (record, last_modified) in records.unwrap_or_default() {
                    let id = record.id.clone();
                    let crt = if let Some(remote_client) = remote_clients.get(&id) {
                        ClientRemoteTabs::from_record_with_remote_client(
                            remote_client
                                .fxa_device_id
                                .as_ref()
                                .unwrap_or(&id)
                                .to_owned(),
                            last_modified,
                            remote_client,
                            record,
                        )
                    } else {
                        // A record with a device that's not in our remote clients seems unlikely, but
                        // could happen - in most cases though, it will be due to a disconnected client -
                        // so we really should consider just dropping it? (Sadly though, it does seem
                        // possible it's actually a very recently connected client, so we keep it)
                        log::info!(
                        "Storing tabs from a client that doesn't appear in the devices list: {}",
                        id,
                    );
                        ClientRemoteTabs::from_record(id, last_modified, record)
                    };
                    crts.push(crt);
                }
                Some(crts)
            }
        }
    }

    pub fn remove_stale_clients(&mut self) -> Result<()> {
        let last_sync = self.get_meta::<i64>(schema::LAST_SYNC_META_KEY)?;
        if let Some(conn) = self.open_if_exists()? {
            if let Some(last_sync) = last_sync {
                let tx = conn.unchecked_transaction()?;
                // Get rid of anything older than 3 weeks of our last sync
                tx.execute_cached(
                    "DELETE FROM tabs WHERE last_modified <= :last_sync - :ttl",
                    rusqlite::named_params! {
                        ":last_sync": last_sync,
                        ":ttl": TTL_3_WEEKS,
                    },
                )?;
                tx.commit()?;
            }
        }
        Ok(())
    }
}

impl TabsStorage {
    pub(crate) fn replace_remote_tabs(
        &mut self,
        // This is a tuple because we need to know what the server reports
        // as the last time a record was modified
        new_remote_tabs: Vec<(TabsRecord, ServerTimestamp)>,
    ) -> Result<()> {
        let connection = self.open_or_create()?;
        let tx = connection.unchecked_transaction()?;

        // For tabs it's fine if we override the existing tabs for a remote
        // there can only ever be one record for each client
        for remote_tab in new_remote_tabs {
            let record = remote_tab.0;
            let last_modified = remote_tab.1;
            tx.execute_cached(
                "INSERT OR REPLACE INTO tabs (guid, record, last_modified) VALUES (:guid, :record, :last_modified);",
                rusqlite::named_params! {
                    ":guid": &record.id,
                    ":record": serde_json::to_string(&record).expect("tabs don't fail to serialize"),
                    ":last_modified": last_modified.as_millis()
                },
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn wipe_remote_tabs(&mut self) -> Result<()> {
        if let Some(db) = self.open_if_exists()? {
            db.execute_batch("DELETE FROM tabs")?;
        }
        Ok(())
    }

    pub(crate) fn wipe_local_tabs(&self) {
        self.local_tabs.replace(None);
    }

    pub(crate) fn put_meta(&mut self, key: &str, value: &dyn ToSql) -> Result<()> {
        if let Some(db) = self.open_if_exists()? {
            db.execute_cached(
                "REPLACE INTO moz_meta (key, value) VALUES (:key, :value)",
                &[(":key", &key as &dyn ToSql), (":value", value)],
            )?;
        }
        Ok(())
    }

    pub(crate) fn get_meta<T: FromSql>(&mut self, key: &str) -> Result<Option<T>> {
        match self.open_if_exists() {
            Ok(Some(db)) => {
                let res = db.try_query_one(
                    "SELECT value FROM moz_meta WHERE key = :key",
                    &[(":key", &key)],
                    true,
                )?;
                Ok(res)
            }
            Err(e) => Err(e),
            Ok(None) => Ok(None),
        }
    }

    pub(crate) fn delete_meta(&mut self, key: &str) -> Result<()> {
        if let Some(db) = self.open_if_exists()? {
            db.execute_cached("DELETE FROM moz_meta WHERE key = :key", &[(":key", &key)])?;
        }
        Ok(())
    }
}

// Try to keep in sync with https://searchfox.org/mozilla-central/rev/2ad13433da20a0749e1e9a10ec0ab49b987c2c8e/modules/libpref/init/all.js#3927
fn is_url_syncable(url: &str) -> bool {
    url.len() <= URI_LENGTH_MAX
        && !(url.starts_with("about:")
            || url.starts_with("resource:")
            || url.starts_with("chrome:")
            || url.starts_with("wyciwyg:")
            || url.starts_with("blob:")
            || url.starts_with("file:")
            || url.starts_with("moz-extension:")
            || url.starts_with("data:"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::record::TabsRecordTab;

    #[test]
    fn test_is_url_syncable() {
        assert!(is_url_syncable("https://bobo.com"));
        assert!(is_url_syncable("ftp://bobo.com"));
        assert!(!is_url_syncable("about:blank"));
        // XXX - this smells wrong - we should insist on a valid complete URL?
        assert!(is_url_syncable("aboutbobo.com"));
        assert!(!is_url_syncable("file:///Users/eoger/bobo"));
    }

    #[test]
    fn test_open_if_exists_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let db_name = dir.path().join("test_open_for_read_no_file.db");
        let mut storage = TabsStorage::new(db_name.clone());
        assert!(storage.open_if_exists().unwrap().is_none());
        storage.open_or_create().unwrap(); // will have created it.
                                           // make a new storage, but leave the file alone.
        let mut storage = TabsStorage::new(db_name);
        // db file exists, so opening for read should open it.
        assert!(storage.open_if_exists().unwrap().is_some());
    }

    #[test]
    fn test_tabs_meta() {
        let mut db = TabsStorage::new_with_mem_path("test");
        let test_key = "TEST KEY A";
        let test_value = "TEST VALUE A";
        let test_key2 = "TEST KEY B";
        let test_value2 = "TEST VALUE B";

        db.put_meta(test_key, &test_value).unwrap();
        db.put_meta(test_key2, &test_value2).unwrap();

        let retrieved_value: String = db.get_meta(test_key).unwrap().expect("test value");
        let retrieved_value2: String = db.get_meta(test_key2).unwrap().expect("test value 2");

        assert_eq!(retrieved_value, test_value);
        assert_eq!(retrieved_value2, test_value2);

        // check that the value of an existing key can be updated
        let test_value3 = "TEST VALUE C";
        db.put_meta(test_key, &test_value3).unwrap();

        let retrieved_value3: String = db.get_meta(test_key).unwrap().expect("test value 3");

        assert_eq!(retrieved_value3, test_value3);

        // check that a deleted key is not retrieved
        db.delete_meta(test_key).unwrap();
        let retrieved_value4: Option<String> = db.get_meta(test_key).unwrap();
        assert!(retrieved_value4.is_none());
    }

    #[test]
    fn test_prepare_local_tabs_for_upload() {
        let mut storage = TabsStorage::new_with_mem_path("test_prepare_local_tabs_for_upload");
        assert_eq!(storage.prepare_local_tabs_for_upload(), None);
        storage.update_local_state(vec![
            RemoteTab {
                title: "".to_owned(),
                url_history: vec!["about:blank".to_owned(), "https://foo.bar".to_owned()],
                icon: None,
                last_used: 0,
            },
            RemoteTab {
                title: "".to_owned(),
                url_history: vec![
                    "https://foo.bar".to_owned(),
                    "about:blank".to_owned(),
                    "about:blank".to_owned(),
                    "about:blank".to_owned(),
                    "about:blank".to_owned(),
                    "about:blank".to_owned(),
                    "about:blank".to_owned(),
                    "about:blank".to_owned(),
                ],
                icon: None,
                last_used: 0,
            },
            RemoteTab {
                title: "".to_owned(),
                url_history: vec![
                    "https://foo.bar".to_owned(),
                    "about:blank".to_owned(),
                    "https://foo2.bar".to_owned(),
                    "https://foo3.bar".to_owned(),
                    "https://foo4.bar".to_owned(),
                    "https://foo5.bar".to_owned(),
                    "https://foo6.bar".to_owned(),
                ],
                icon: None,
                last_used: 0,
            },
            RemoteTab {
                title: "".to_owned(),
                url_history: vec![],
                icon: None,
                last_used: 0,
            },
        ]);
        assert_eq!(
            storage.prepare_local_tabs_for_upload(),
            Some(vec![
                RemoteTab {
                    title: "".to_owned(),
                    url_history: vec!["https://foo.bar".to_owned()],
                    icon: None,
                    last_used: 0,
                },
                RemoteTab {
                    title: "".to_owned(),
                    url_history: vec![
                        "https://foo.bar".to_owned(),
                        "https://foo2.bar".to_owned(),
                        "https://foo3.bar".to_owned(),
                        "https://foo4.bar".to_owned(),
                        "https://foo5.bar".to_owned()
                    ],
                    icon: None,
                    last_used: 0,
                },
            ])
        );
    }
    // Helper struct to model what's stored in the DB
    struct TabsSQLRecord {
        guid: String,
        record: TabsRecord,
        last_modified: i64,
    }
    #[test]
    fn test_remove_stale_clients() {
        let dir = tempfile::tempdir().unwrap();
        let db_name = dir.path().join("test_remove_stale_clients.db");
        let mut storage = TabsStorage::new(db_name);
        storage.open_or_create().unwrap();
        assert!(storage.open_if_exists().unwrap().is_some());

        let records = vec![
            TabsSQLRecord {
                guid: "device-1".to_string(),
                record: TabsRecord {
                    id: "device-1".to_string(),
                    client_name: "Device #1".to_string(),
                    tabs: vec![TabsRecordTab {
                        title: "the title".to_string(),
                        url_history: vec!["https://mozilla.org/".to_string()],
                        icon: Some("https://mozilla.org/icon".to_string()),
                        last_used: 1643764207,
                    }],
                },
                last_modified: 1643764207,
            },
            TabsSQLRecord {
                guid: "device-outdated".to_string(),
                record: TabsRecord {
                    id: "device-outdated".to_string(),
                    client_name: "Device outdated".to_string(),
                    tabs: vec![TabsRecordTab {
                        title: "the title".to_string(),
                        url_history: vec!["https://mozilla.org/".to_string()],
                        icon: Some("https://mozilla.org/icon".to_string()),
                        last_used: 1643764207,
                    }],
                },
                last_modified: 1443764207, // old
            },
        ];
        let db = storage.open_if_exists().unwrap().unwrap();
        for record in records {
            db.execute(
                "INSERT INTO tabs (guid, record, last_modified) VALUES (:guid, :record, :last_modified);",
                rusqlite::named_params! {
                    ":guid": &record.guid,
                    ":record": serde_json::to_string(&record.record).unwrap(),
                    ":last_modified": &record.last_modified,
                },
            ).unwrap();
        }
        // pretend we just synced
        let last_synced = 1643764207_i64;
        storage
            .put_meta(schema::LAST_SYNC_META_KEY, &last_synced)
            .unwrap();
        storage.remove_stale_clients().unwrap();

        let remote_tabs = storage.get_remote_tabs().unwrap();
        // We should've removed the outdated device
        assert_eq!(remote_tabs.len(), 1);
    }
}
