/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::sync::engine::LoginsSyncEngine;
use crate::LoginStore;
use anyhow::Result;
use std::sync::Arc;
use sync15::bso::{IncomingBso, OutgoingBso};
use sync15::engine::{BridgedEngine, BridgedEngineAdaptor};
use sync15::ServerTimestamp;
use sync_guid::Guid as SyncGuid;

impl LoginStore {
    /// Returns a bridged sync engine for Desktop for this store.
    ///
    /// Unlike Tabs, constructing a `LoginsSyncEngine` locks the DB and can
    /// fail, so this is fallible (and exposed as `[Throws]` in the UDL). The
    /// internal error is surfaced via `anyhow`, which UniFFI maps onto
    /// `LoginsApiError` through `From<anyhow::Error>`.
    pub fn bridged_engine(self: Arc<Self>) -> Result<Arc<LoginsBridgedEngine>> {
        let engine = LoginsSyncEngine::new(self)?;
        let bridged_engine = LoginsBridgedEngineAdaptor { engine };
        Ok(Arc::new(LoginsBridgedEngine::new(Box::new(bridged_engine))))
    }
}

/// `LoginsSyncEngine` only implements the internal `sync15::SyncEngine` trait,
/// which is what the mobile (Android/iOS) sync manager drives. Desktop's Sync
/// framework instead speaks the `mozIBridgedSyncEngine` interface, whose Rust
/// shape is `sync15::BridgedEngine`. This adaptor wraps our `SyncEngine` and,
/// via the blanket `impl<A: BridgedEngineAdaptor> BridgedEngine for A`, gives
/// us a `BridgedEngine` for free. The adaptor exists only because these two
/// sync-engine traits still live side by side; it can go away if they're ever
/// unified.
struct LoginsBridgedEngineAdaptor {
    engine: LoginsSyncEngine,
}

/// see sync15/src/engine/bridged_engine.rs for required functions for the trait
impl BridgedEngineAdaptor for LoginsBridgedEngineAdaptor {
    fn last_sync(&self) -> Result<i64> {
        // `get_last_sync` takes the `&LoginDb` to avoid deadlocking when called
        // mid-sync (while the lock is already held). The bridge methods are
        // always called outside a sync transaction, so we can lock here.
        let db = self.engine.store.lock_db()?;
        Ok(self
            .engine
            .get_last_sync(&db)?
            .unwrap_or_default()
            .as_millis())
    }

    fn set_last_sync(&self, last_sync_millis: i64) -> Result<()> {
        let db = self.engine.store.lock_db()?;
        self.engine
            .set_last_sync(&db, ServerTimestamp::from_millis(last_sync_millis))?;
        Ok(())
    }

    fn engine(&self) -> &dyn sync15::engine::SyncEngine {
        &self.engine
    }
}

// This is what UniFFI exposes; it does nothing other than delegate back to the
// `BridgedEngine` trait object (and handle the JSON (de)serialization of BSOs
// that crosses the FFI boundary).
/// see services/interfaces/mozIBridgedSyncEngine.idl for contract
pub struct LoginsBridgedEngine {
    bridge_impl: Box<dyn BridgedEngine>,
}

impl LoginsBridgedEngine {
    pub fn new(bridge_impl: Box<dyn BridgedEngine>) -> Self {
        Self { bridge_impl }
    }

    pub fn last_sync(&self) -> Result<i64> {
        self.bridge_impl.last_sync()
    }

    pub fn set_last_sync(&self, last_sync: i64) -> Result<()> {
        self.bridge_impl.set_last_sync(last_sync)
    }

    pub fn sync_id(&self) -> Result<Option<String>> {
        self.bridge_impl.sync_id()
    }

    pub fn reset_sync_id(&self) -> Result<String> {
        self.bridge_impl.reset_sync_id()
    }

    pub fn ensure_current_sync_id(&self, sync_id: &str) -> Result<String> {
        self.bridge_impl.ensure_current_sync_id(sync_id)
    }

    pub fn sync_started(&self) -> Result<()> {
        self.bridge_impl.sync_started()
    }

    // Decode the JSON-encoded IncomingBso's that UniFFI passes to us
    fn convert_incoming_bsos(&self, incoming: Vec<String>) -> Result<Vec<IncomingBso>> {
        let mut bsos = Vec::with_capacity(incoming.len());
        for inc in incoming {
            bsos.push(serde_json::from_str::<IncomingBso>(&inc)?);
        }
        Ok(bsos)
    }

    // Encode OutgoingBso's into JSON for UniFFI
    fn convert_outgoing_bsos(&self, outgoing: Vec<OutgoingBso>) -> Result<Vec<String>> {
        let mut bsos = Vec::with_capacity(outgoing.len());
        for e in outgoing {
            bsos.push(serde_json::to_string(&e)?);
        }
        Ok(bsos)
    }

    pub fn store_incoming(&self, incoming: Vec<String>) -> Result<()> {
        self.bridge_impl
            .store_incoming(self.convert_incoming_bsos(incoming)?)
    }

    pub fn apply(&self) -> Result<Vec<String>> {
        let apply_results = self.bridge_impl.apply()?;
        self.convert_outgoing_bsos(apply_results.records)
    }

    pub fn set_uploaded(&self, server_modified_millis: i64, guids: Vec<String>) -> Result<()> {
        // UniFFI hands us plain strings; the bridge works in terms of `Guid`.
        let guids: Vec<SyncGuid> = guids.into_iter().map(SyncGuid::from).collect();
        self.bridge_impl
            .set_uploaded(server_modified_millis, &guids)
    }

    pub fn sync_finished(&self) -> Result<()> {
        self.bridge_impl.sync_finished()
    }

    pub fn reset(&self) -> Result<()> {
        self.bridge_impl.reset()
    }

    pub fn wipe(&self) -> Result<()> {
        self.bridge_impl.wipe()
    }
}

#[cfg(not(feature = "keydb"))]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_utils::insert_login;
    use nss_as::ensure_initialized;
    use std::collections::HashMap;

    // Exercises the sync-metadata plumbing (last_sync / sync_id / reset) that
    // Desktop's Sync framework drives through the bridge, mirroring the Tabs
    // `test_sync_meta` test.
    #[test]
    fn test_sync_meta() {
        ensure_initialized();
        error_support::init_for_tests();

        let store = Arc::new(LoginStore::new_in_memory());
        let bridge = store.bridged_engine().expect("should create bridge");

        // Fresh DB: never synced.
        assert_eq!(bridge.last_sync().unwrap(), 0);
        bridge.set_last_sync(3).unwrap();
        assert_eq!(bridge.last_sync().unwrap(), 3);

        assert!(bridge.sync_id().unwrap().is_none());

        bridge.ensure_current_sync_id("some_guid").unwrap();
        assert_eq!(bridge.sync_id().unwrap(), Some("some_guid".to_string()));
        // changing the sync ID should reset the timestamp
        assert_eq!(bridge.last_sync().unwrap(), 0);
        bridge.set_last_sync(3).unwrap();

        bridge.reset_sync_id().unwrap();
        // should now be a random guid.
        assert_ne!(bridge.sync_id().unwrap(), Some("some_guid".to_string()));
        // should have reset the last sync timestamp.
        assert_eq!(bridge.last_sync().unwrap(), 0);
        bridge.set_last_sync(3).unwrap();

        // `reset` clears the guid and the timestamp
        bridge.reset().unwrap();
        assert_eq!(bridge.last_sync().unwrap(), 0);
        assert!(bridge.sync_id().unwrap().is_none());
    }

    // A roundtrip through the bridge's data path: stage an incoming remote
    // login, apply it, and confirm the local-only login comes back out for
    // upload. Unlike `test_sync_meta`, this exercises the JSON (de)serialization
    // of BSOs and the staged-incoming `Mutex`. Mirrors the Tabs
    // `test_sync_via_bridge` test.
    #[test]
    fn test_sync_via_bridge() {
        ensure_initialized();
        error_support::init_for_tests();

        let store = Arc::new(LoginStore::new_in_memory());

        // A local-only login: nothing on the server knows about it yet, so it
        // should be uploaded.
        insert_login(
            &store.lock_db().unwrap(),
            "local-only-aaaa",
            Some("local-password"),
            None,
        );

        let bridge = store
            .clone()
            .bridged_engine()
            .expect("should create bridge");

        bridge.sync_started().unwrap();

        // An incoming remote login that isn't known locally. We build the
        // envelope as raw JSON, exactly as the JS bridge hands it to us.
        let incoming = vec![serde_json::json!({
            "id": "remote-only-bbbb",
            "modified": 0,
            "payload": serde_json::json!({
                "id": "remote-only-bbbb",
                "hostname": "https://remote.example.com",
                "formSubmitURL": "https://remote.example.com",
                "username": "remote-user",
                "password": "remote-password",
            })
            .to_string(),
        })
        .to_string()];
        bridge
            .store_incoming(incoming)
            .expect("should store incoming");

        // Applying stores the remote record locally and returns the local-only
        // login for upload.
        let outgoing = bridge.apply().expect("should apply");
        let changes: HashMap<String, serde_json::Value> = outgoing
            .into_iter()
            .map(|s| {
                let bso: serde_json::Value = serde_json::from_str(&s).unwrap();
                let payload: serde_json::Value =
                    serde_json::from_str(bso["payload"].as_str().unwrap()).unwrap();
                (payload["id"].as_str().unwrap().to_string(), payload)
            })
            .collect();

        // Only the local login is outgoing; the just-applied remote one is not
        // re-uploaded.
        assert_eq!(changes.len(), 1);
        assert_eq!(changes["local-only-aaaa"]["password"], "local-password");

        // The incoming remote login was actually persisted.
        let stored = store
            .get("remote-only-bbbb")
            .unwrap()
            .expect("remote login should have been stored");
        assert_eq!(stored.password, "remote-password");

        // Acknowledging the upload advances last_sync.
        bridge
            .set_uploaded(1234, vec!["local-only-aaaa".to_string()])
            .unwrap();
        bridge.sync_finished().unwrap();
        assert_eq!(bridge.last_sync().unwrap(), 1234);
    }
}
