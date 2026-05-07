/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Weak},
};

use camino::Utf8PathBuf;
use error_support::trace;
use parking_lot::Mutex;
use serde::Deserialize;
use url::Url;
use viaduct::Request;

use crate::{
    client::RemoteState, config::BaseUrl, error::Error, storage::Storage, RemoteSettingsClient,
    RemoteSettingsConfig2, RemoteSettingsContext, RemoteSettingsServer, Result,
};

/// Internal Remote settings service API
pub struct RemoteSettingsService {
    inner: Mutex<RemoteSettingsServiceInner>,
}

struct RemoteSettingsServiceInner {
    storage_dir: Utf8PathBuf,
    base_url: BaseUrl,
    bucket_name: String,
    app_context: Option<RemoteSettingsContext>,
    remote_state: RemoteState,
    /// Weakrefs for all clients that we've created.  Note: this stores the
    /// top-level/public `RemoteSettingsClient` structs rather than `client::RemoteSettingsClient`.
    /// The reason for this is that we return Arcs to the public struct to the foreign code, so we
    /// need to use the same type for our weakrefs.  The alternative would be to create 2 Arcs for
    /// each client, which is wasteful.
    clients: Vec<Weak<RemoteSettingsClient>>,
}

impl RemoteSettingsService {
    /// Construct a [RemoteSettingsService]
    ///
    /// This is typically done early in the application-startup process
    pub fn new(storage_dir: String, config: RemoteSettingsConfig2) -> Self {
        let storage_dir = storage_dir.into();
        let base_url = config
            .server
            .unwrap_or(RemoteSettingsServer::Prod)
            .get_base_url_with_prod_fallback();
        let bucket_name = config.bucket_name.unwrap_or_else(|| String::from("main"));

        Self {
            inner: Mutex::new(RemoteSettingsServiceInner {
                storage_dir,
                base_url,
                bucket_name,
                app_context: config.app_context,
                remote_state: RemoteState::default(),
                clients: vec![],
            }),
        }
    }

    pub fn make_client(&self, collection_name: String) -> Arc<RemoteSettingsClient> {
        let mut inner = self.inner.lock();
        // Allow using in-memory databases for testing of external crates.
        let storage = if inner.storage_dir == ":memory:" {
            Storage::new(inner.storage_dir.clone())
        } else {
            Storage::new(inner.storage_dir.join(format!("{collection_name}.sql")))
        };

        let client = Arc::new(RemoteSettingsClient::new(
            inner.base_url.clone(),
            inner.bucket_name.clone(),
            collection_name.clone(),
            inner.app_context.clone(),
            storage,
        ));
        inner.clients.push(Arc::downgrade(&client));
        client
    }

    /// Sync collections for all active clients
    pub fn sync(&self) -> Result<Vec<String>> {
        // Make sure we only sync each collection once, even if there are multiple clients
        let mut synced_collections = HashSet::new();

        let mut inner = self.inner.lock();
        let changes = inner.fetch_changes()?;
        let change_map: HashMap<_, _> = changes
            .changes
            .iter()
            .map(|c| ((c.collection.as_str(), &c.bucket), c.last_modified))
            .collect();
        let bucket_name = inner.bucket_name.clone();

        let active_clients = inner.active_clients();
        for client in &active_clients {
            let client = &client.internal;
            let collection_name = client.collection_name();
            if let Some(client_last_modified) = client.get_last_modified_timestamp()? {
                if let Some(server_last_modified) = change_map.get(&(collection_name, &bucket_name))
                {
                    if client_last_modified == *server_last_modified {
                        trace!("skipping up-to-date collection: {collection_name}");
                        continue;
                    }
                }
            }
            if synced_collections.insert(collection_name.to_string()) {
                trace!("syncing collection: {collection_name}");
                client.sync()?;
            }
        }

        // Run SQLite maintenance after sync so SQLite can reclaim pages freed by
        // attachment cleanup and enable/use incremental auto-vacuum.
        for client in &active_clients {
            let client = &client.internal;
            let collection_name = client.collection_name();

            if synced_collections.contains(collection_name) {
                trace!("running maintenance for collection: {collection_name}");
                client.run_maintenance()?;
            }
        }

        Ok(synced_collections.into_iter().collect())
    }

    /// Update the remote settings config
    ///
    /// This will cause all current and future clients to use new config and will delete any stored
    /// records causing the clients to return new results from the new config.
    pub fn update_config(&self, config: RemoteSettingsConfig2) -> Result<()> {
        let base_url = config
            .server
            .unwrap_or(RemoteSettingsServer::Prod)
            .get_base_url()?;
        let bucket_name = config.bucket_name.unwrap_or_else(|| String::from("main"));
        let mut inner = self.inner.lock();
        for client in inner.active_clients() {
            client.internal.update_config(
                base_url.clone(),
                bucket_name.clone(),
                config.app_context.clone(),
            );
        }
        inner.base_url = base_url;
        inner.bucket_name = bucket_name;
        inner.app_context = config.app_context;
        Ok(())
    }

    pub fn client_url(&self) -> Url {
        let inner = self.inner.lock();
        let base_url = inner.base_url.clone();
        base_url.url().clone()
    }
}

impl RemoteSettingsServiceInner {
    // Find live clients in self.clients
    //
    // Also, drop dead weakrefs from the vec
    fn active_clients(&mut self) -> Vec<Arc<RemoteSettingsClient>> {
        let mut active_clients = vec![];
        self.clients.retain(|weak| {
            if let Some(client) = weak.upgrade() {
                active_clients.push(client);
                true
            } else {
                false
            }
        });
        active_clients
    }

    fn fetch_changes(&mut self) -> Result<Changes> {
        let mut url = self.base_url.clone();
        url.path_segments_mut()
            .push("buckets")
            .push("monitor")
            .push("collections")
            .push("changes")
            .push("changeset");
        // For now, always use `0` for the expected value.  This means we'll get updates based on
        // the default TTL of 1 hour.
        //
        // Eventually, we should add support for push notifications and use the timestamp from the
        // notification.
        url.query_pairs_mut().append_pair("_expected", "0");
        let url = url.into_inner();
        trace!("make_request: {url}");
        self.remote_state.ensure_no_backoff()?;

        let req = Request::get(url);
        let resp = req.send()?;

        self.remote_state.handle_backoff_hint(&resp)?;

        if resp.is_success() {
            Ok(resp.json()?)
        } else {
            Err(Error::response_error(
                &resp.url,
                format!("status code: {}", resp.status),
            ))
        }
    }
}

/// Data from the changes endpoint
///
/// https://remote-settings.readthedocs.io/en/latest/client-specifications.html#endpoints
#[derive(Debug, Deserialize)]
struct Changes {
    changes: Vec<ChangesCollection>,
}

#[derive(Debug, Deserialize)]
struct ChangesCollection {
    collection: String,
    bucket: String,
    last_modified: u64,
}

#[cfg(all(test, not(feature = "signatures")))]
mod test {
    use super::*;
    use mockito::{mock, Matcher};

    #[cfg(not(feature = "signatures"))]
    #[test]
    fn test_sync_maintenance_shrinks_db_after_attachment_cleanup() -> Result<()> {
        use crate::RemoteSettingsRecord;
        use sha2::Digest;
        viaduct_dev::init_backend_dev();

        let collection = "cid";
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let db_path = temp_dir.path().join(format!("{collection}.sql"));

        let attachment_data = vec![0x41; 5 * 1024 * 1024];
        let attachment_hash = format!("{:x}", sha2::Sha256::digest(&attachment_data));

        let attachment_record = format!(
            r#"{{
                "id": "record-with-attachment",
                "last_modified": 100,
                "attachment": {{
                    "filename": "big.bin",
                    "mimetype": "application/octet-stream",
                    "location": "attachments/big.bin",
                    "hash": "{attachment_hash}",
                    "size": {}
                }}
            }}"#,
            attachment_data.len()
        );

        // First sync creates a record that references the big attachment.
        let _changes_1 = mock("GET", "/v1/buckets/monitor/collections/changes/changeset")
            .match_query(Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                r#"{{
                    "timestamp": 100,
                    "changes": [
                        {{"collection": "{collection}", "bucket": "main", "last_modified": 100}}
                    ]
                }}"#
            ))
            .create();

        let _changeset_1 = mock(
            "GET",
            format!("/v1/buckets/main/collections/{collection}/changeset").as_str(),
        )
        .match_query(Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(format!(
            r#"{{
                "changes": [{attachment_record}],
                "timestamp": 100,
                "metadata": {{"bucket": "main", "signatures": []}}
            }}"#
        ))
        .create();

        let service = RemoteSettingsService::new(
            temp_dir.path().to_string_lossy().to_string(),
            RemoteSettingsConfig2 {
                server: Some(RemoteSettingsServer::Custom {
                    url: mockito::server_url(),
                }),
                ..Default::default()
            },
        );

        let client = service.make_client(collection.into());

        service.sync()?;

        // Mock attachment discovery and download.
        let _root = mock("GET", "/v1/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                r#"{{
                    "capabilities": {{
                        "attachments": {{
                            "base_url": "{}/"
                        }}
                    }}
                }}"#,
                mockito::server_url()
            ))
            .create();

        // Path matches `location: "attachments/big"` joined against the base URL above.
        let _attachment = mock("GET", "/attachments/big")
            .with_status(200)
            .with_body(attachment_data.clone())
            .create();

        // Store the large attachment so the DB becomes bloated.
        client.internal.get_attachment(&RemoteSettingsRecord {
            id: "record-with-attachment".to_string(),
            last_modified: 100,
            deleted: false,
            attachment: Some(crate::Attachment {
                filename: "big".to_string(),
                mimetype: "application/octet-stream".to_string(),
                location: "attachments/big".to_string(),
                hash: attachment_hash.clone(),
                size: attachment_data.len() as u64,
            }),
            fields: serde_json::Map::new(),
        })?;

        let size_with_attachment = std::fs::metadata(&db_path)
            .expect("db exists after first sync")
            .len();

        assert!(
            size_with_attachment > 4 * 1024 * 1024,
            "DB should contain the large attachment; size={size_with_attachment}"
        );

        // Drop first-sync mocks explicitly so mockito doesn't re-match the second sync's
        // changeset request against them. Mockito matches by registration order, so leftover
        // mocks for the same URL would shadow the second-sync mocks.
        drop(_changes_1);
        drop(_changeset_1);

        // Second sync tombstones the record. This deletes the attachment row, and
        // post-sync maintenance should compact the database.
        let _changes_2 = mock("GET", "/v1/buckets/monitor/collections/changes/changeset")
            .match_query(Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                r#"{{
                    "timestamp": 200,
                    "changes": [
                        {{"collection": "{collection}", "bucket": "main", "last_modified": 200}}
                    ]
                }}"#
            ))
            .create();

        let _changeset_2 = mock(
            "GET",
            format!("/v1/buckets/main/collections/{collection}/changeset").as_str(),
        )
        .match_query(Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{
                "changes": [
                    {
                        "id": "record-with-attachment",
                        "last_modified": 200,
                        "deleted": true
                    }
                ],
                "timestamp": 200,
                "metadata": {"bucket": "main", "signatures": []}
            }"#,
        )
        .create();

        service.sync()?;

        let size_after_cleanup_and_maintenance = std::fs::metadata(&db_path)
            .expect("db exists after second sync")
            .len();

        assert!(
            size_after_cleanup_and_maintenance < size_with_attachment,
            "maintenance should reclaim at least some space after deleting attachment; before={size_with_attachment}, after={size_after_cleanup_and_maintenance}"
        );

        // Sanity-check that maintenance enabled incremental auto-vacuum.
        let conn = rusqlite::Connection::open(&db_path).expect("open collection db");
        let auto_vacuum: u32 = conn
            .query_row("PRAGMA auto_vacuum", [], |row| row.get(0))
            .expect("query auto_vacuum");

        assert_eq!(auto_vacuum, 2);

        Ok(())
    }
}
