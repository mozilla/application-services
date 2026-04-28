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
    client::RemoteState, config::BaseUrl, error::Error, storage::Storage,
    telemetry::RemoteSettingsTelemetryWrapper, RemoteSettingsClient, RemoteSettingsConfig,
    RemoteSettingsContext, RemoteSettingsServer, Result,
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
    telemetry: RemoteSettingsTelemetryWrapper,
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
    pub fn new(storage_dir: String, config: RemoteSettingsConfig) -> Self {
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
                telemetry: RemoteSettingsTelemetryWrapper::noop(),
                clients: vec![],
            }),
        }
    }

    pub fn set_telemetry(&self, telemetry: RemoteSettingsTelemetryWrapper) {
        self.inner.lock().telemetry = telemetry;
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
            let cid = format!("{bucket_name}/{collection_name}");
            if let Some(client_last_modified) = client.get_last_modified_timestamp()? {
                if let Some(server_last_modified) = change_map.get(&(collection_name, &bucket_name))
                {
                    if client_last_modified == *server_last_modified {
                        trace!("skipping up-to-date collection: {collection_name}");
                        inner.telemetry.report_uptake_up_to_date(&cid, None);
                        continue;
                    }
                }
            }
            if synced_collections.insert(collection_name.to_string()) {
                trace!("syncing collection: {collection_name}");
                let start_time = std::time::Instant::now();
                let sync_result = client.sync();
                let duration: u64 = start_time.elapsed().as_millis().try_into().unwrap_or(0);
                match &sync_result {
                    Ok(()) => inner.telemetry.report_uptake_success(&cid, Some(duration)),
                    Err(e) => inner.telemetry.report_uptake_error(e, &cid),
                }
                sync_result?;
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
    pub fn update_config(&self, config: RemoteSettingsConfig) -> Result<()> {
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

        let start_time = std::time::Instant::now();
        let req = Request::get(url);
        let resp = req.send()?;

        self.remote_state.handle_backoff_hint(&resp)?;

        const TELEMETRY_SOURCE_POLL: &str = "settings-changes-monitoring";
        if resp.is_success() {
            let body = resp.json()?;
            let duration: u64 = start_time.elapsed().as_millis().try_into().unwrap_or(0);
            self.telemetry
                .report_uptake_success(TELEMETRY_SOURCE_POLL, Some(duration));
            Ok(body)
        } else {
            let e = Error::response_error(&resp.url, format!("status code: {}", resp.status));
            self.telemetry
                .report_uptake_error(&e, TELEMETRY_SOURCE_POLL);
            Err(e)
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::telemetry::UptakeEventExtras;
    use crate::{RemoteSettingsConfig, RemoteSettingsServer};
    use mockito::{mock, Matcher};
    use std::sync::Arc;

    /// Telemetry implementation that records all events for later assertion.
    struct FakeTelemetry {
        events: std::sync::Mutex<Vec<UptakeEventExtras>>,
    }

    impl FakeTelemetry {
        fn new() -> Self {
            Self {
                events: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    impl crate::telemetry::RemoteSettingsTelemetry for FakeTelemetry {
        fn report_uptake(&self, extras: UptakeEventExtras) {
            self.events.lock().unwrap().push(extras);
        }
    }

    fn make_service(server_url: &str) -> (RemoteSettingsService, Arc<FakeTelemetry>) {
        let service = RemoteSettingsService::new(
            ":memory:".into(),
            RemoteSettingsConfig {
                server: Some(RemoteSettingsServer::Custom {
                    url: server_url.into(),
                }),
                ..Default::default()
            },
        );
        let telemetry: Arc<FakeTelemetry> = Arc::new(FakeTelemetry::new());
        service.set_telemetry(RemoteSettingsTelemetryWrapper::new(telemetry.clone()));
        (service, telemetry)
    }

    fn mock_monitor_changes(collection: &str, timestamp: u64) -> mockito::Mock {
        mock("GET", "/v1/buckets/monitor/collections/changes/changeset")
            .match_query(Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                r#"{{"timestamp": {timestamp}, "changes": [{{"collection": "{collection}", "bucket": "main", "last_modified": {timestamp}}}]}}"#
            ))
            .create()
    }

    fn mock_changeset(collection: &str, timestamp: u64) -> mockito::Mock {
        mock(
            "GET",
            format!("/v1/buckets/main/collections/{collection}/changeset").as_str(),
        )
        .match_query(Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(format!(
            r#"{{"changes": [], "timestamp": {timestamp}, "metadata": {{"bucket": "main", "signatures": []}}}}"#
        ))
        .create()
    }

    fn mock_changeset_error(bucket: &str, collection: &str) -> mockito::Mock {
        mock(
            "GET",
            format!("/v1/buckets/{bucket}/collections/{collection}/changeset").as_str(),
        )
        .match_query(Matcher::Any)
        .with_status(500)
        .with_body("server error")
        .create()
    }

    #[test]
    fn test_telemetry_network_error_on_changes_failure() {
        viaduct_dev::init_backend_dev();
        mock_changeset_error("monitor", "changes");

        let (service, telemetry) = make_service(&mockito::server_url());
        let _ = service.sync();

        let events = telemetry.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].source,
            Some("settings-changes-monitoring".to_string())
        );
        assert_eq!(events[0].value, Some("network_error".to_string()));
        assert_eq!(events[0].error_name, Some("ResponseError".to_string()));
        assert!(events[0].error_name.is_some());
    }

    #[test]
    fn test_telemetry_on_changes_success() {
        viaduct_dev::init_backend_dev();
        let _changes = mock_monitor_changes("cid", 42);

        let (service, telemetry) = make_service(&mockito::server_url());
        let _ = service.sync();

        let events = telemetry.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].source,
            Some("settings-changes-monitoring".to_string())
        );
        assert_eq!(events[0].value, Some("success".to_string()));
        assert!(events[0].duration.is_some());
    }

    #[cfg(not(feature = "signatures"))]
    #[test]
    fn test_telemetry_on_collection_success() {
        viaduct_dev::init_backend_dev();
        let collection = "cid";
        let timestamp = 1774420582054u64;
        let _changes = mock_monitor_changes(collection, timestamp);
        let _changeset = mock_changeset(collection, timestamp);

        let (service, telemetry) = make_service(&mockito::server_url());
        let _client = service.make_client(collection.into());
        let _ = service.sync();

        let events = telemetry.events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(
            events[0].source,
            Some("settings-changes-monitoring".to_string())
        );
        assert_eq!(events[1].source, Some(format!("main/{collection}")));
        assert_eq!(events[1].value, Some("success".to_string()));
        assert!(events[1].duration.is_some());
    }

    #[cfg(not(feature = "signatures"))]
    #[test]
    fn test_telemetry_on_collection_up_to_date() {
        viaduct_dev::init_backend_dev();
        let collection = "cid";
        let timestamp = 1774420582054u64;
        let _changes = mock_monitor_changes(collection, timestamp);
        let _changeset = mock_changeset(collection, timestamp);

        let (service, telemetry) = make_service(&mockito::server_url());
        let _client = service.make_client(collection.into());

        // First sync: populates local storage with timestamp.
        let _ = service.sync();
        let events_before = telemetry.events.lock().unwrap().len();
        // Second sync.
        let _ = service.sync();

        let events = telemetry.events.lock().unwrap();
        assert_eq!(events.len() - events_before, 2);
        assert_eq!(
            events[events_before].source,
            Some("settings-changes-monitoring".to_string())
        );
        assert_eq!(
            events[events_before + 1].source,
            Some(format!("main/{collection}"))
        );
        assert_eq!(
            events[events_before + 1].value,
            Some("up_to_date".to_string())
        );
    }

    #[test]
    fn test_telemetry_on_collection_error() {
        viaduct_dev::init_backend_dev();
        let collection = "cid";
        let timestamp = 1774420582054u64;
        let _changes = mock_monitor_changes(collection, timestamp);
        let _changeset = mock_changeset_error("main", collection);

        let (service, telemetry) = make_service(&mockito::server_url());
        let _client = service.make_client(collection.into());
        let _ = service.sync();

        let events = telemetry.events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(
            events[0].source,
            Some("settings-changes-monitoring".to_string())
        );
        assert_eq!(events[0].value, Some("success".to_string()));
        assert_eq!(events[1].source, Some(format!("main/{collection}")));
        assert_eq!(events[1].value, Some("network_error".to_string()));
        assert_eq!(events[1].error_name, Some("ResponseError".to_string()));
    }

    #[cfg(feature = "signatures")]
    #[test]
    fn test_telemetry_on_collection_signature_error() {
        viaduct_dev::init_backend_dev();
        let collection = "cid";
        let timestamp = 1774420582054u64;
        let _changes = mock_monitor_changes(collection, timestamp);
        let _changeset = mock_changeset(collection, timestamp);

        let (service, telemetry) = make_service(&mockito::server_url());
        let _client = service.make_client(collection.into());
        let _ = service.sync();

        let events = telemetry.events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(
            events[0].source,
            Some("settings-changes-monitoring".to_string())
        );
        assert_eq!(events[1].source, Some(format!("main/{collection}")));
        assert_eq!(events[1].value, Some("signature_error".to_string()));
        assert_eq!(
            events[1].error_name,
            Some("IncompleteSignatureDataError".to_string())
        );
    }

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
            RemoteSettingsConfig {
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
