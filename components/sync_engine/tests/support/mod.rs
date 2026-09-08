/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Shared harness for the multi-client simulator tests.
//!
//! Plays the two roles the engine doesn't: a shared server ([`FakeServer`]) and
//! the per-client sync loop ([`SimClient::sync`]). Each client owns an
//! [`EventSyncEngine`] backed by a [`MapReducer`] standing in for storage.
//!
//! Convergence holds because every `sync()` downloads-then-uploads and
//! `last_sync` advances monotonically, so each client folds the entire event
//! stream in one global ascending order; a late joiner folds a baseline plus the
//! newer events.
//!
//! Lives in a subdirectory so cargo treats it as a plain module the scenario
//! files (`sim_*.rs`) share, not as its own test binary.

// Each scenario binary uses only part of this harness; suppress the resulting
// per-binary dead-code noise rather than annotate every item.
#![allow(dead_code)]

use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use sync15::bso::OutgoingBso;
use sync15::engine::SyncEngine;
use sync15::{telemetry, Guid, ServerTimestamp};
use sync_engine::{
    BaselineConfig, EventReducer, EventSyncEngine, MemoryMetadataStore, ResolvedEvent,
    SchemaRegistry, SnapshotEntity,
};

// -------------------------------------------------------------------------
// Fake server: an in-memory, GUID-keyed collection with a logical clock.
// -------------------------------------------------------------------------

#[derive(Default)]
pub struct FakeServer {
    /// Monotonic logical clock; each non-empty upload bumps it once and stamps
    /// the whole batch, so a batch shares a `modified` (exercises the tie-break).
    clock: i64,
    records: HashMap<Guid, (OutgoingBso, ServerTimestamp)>,
}

impl FakeServer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn timestamp(&self) -> ServerTimestamp {
        ServerTimestamp::from_millis(self.clock)
    }

    /// Records strictly newer than `newer`, as the client would download them.
    pub fn fetch(&self, newer: Option<ServerTimestamp>) -> Vec<sync15::bso::IncomingBso> {
        let since = newer.map_or(0, |t| t.as_millis());
        self.records
            .values()
            .filter(|(_, ts)| ts.as_millis() > since)
            .map(|(bso, ts)| bso.to_test_incoming_ts(*ts))
            .collect()
    }

    /// Commit an upload batch, assigning it the next clock tick.
    pub fn upload(&mut self, bsos: Vec<OutgoingBso>) -> (ServerTimestamp, Vec<Guid>) {
        if bsos.is_empty() {
            return (self.timestamp(), Vec::new());
        }
        self.clock += 1;
        let ts = ServerTimestamp::from_millis(self.clock);
        let mut ids = Vec::with_capacity(bsos.len());
        for bso in bsos {
            let id = bso.envelope.id.clone();
            ids.push(id.clone());
            self.records.insert(id, (bso, ts));
        }
        (ts, ids)
    }

    pub fn baseline_count(&self) -> usize {
        self.records
            .values()
            .filter(|(bso, _)| {
                serde_json::from_str::<Value>(&bso.payload)
                    .ok()
                    .and_then(|v| {
                        v.get("record")
                            .and_then(Value::as_str)
                            .map(|s| s == "baseline")
                    })
                    .unwrap_or(false)
            })
            .count()
    }
}

// -------------------------------------------------------------------------
// Reducer: a HashMap<entity, payload> standing in for local storage.
// -------------------------------------------------------------------------

/// Cheap-to-clone handle over shared state, so a [`SimClient`] can apply local
/// edits and hand the same store to its engine.
#[derive(Clone, Default)]
pub struct MapReducer {
    state: Arc<Mutex<HashMap<Guid, Value>>>,
}

fn merge(target: &mut Value, src: &Value) {
    if let (Some(t), Some(s)) = (target.as_object_mut(), src.as_object()) {
        for (k, v) in s {
            t.insert(k.clone(), v.clone());
        }
    }
}

impl MapReducer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn state(&self) -> HashMap<Guid, Value> {
        self.state.lock().unwrap().clone()
    }

    pub fn local_create(&self, id: Guid, fields: Value) {
        self.state.lock().unwrap().insert(id, fields);
    }

    pub fn local_merge(&self, id: &Guid, fields: &Value) {
        let mut s = self.state.lock().unwrap();
        let entry = s.entry(id.clone()).or_insert_with(|| json!({}));
        merge(entry, fields);
    }

    pub fn local_remove(&self, id: &Guid) {
        self.state.lock().unwrap().remove(id);
    }
}

impl EventReducer for MapReducer {
    fn snapshot(&self) -> Vec<SnapshotEntity> {
        let s = self.state.lock().unwrap();
        let mut entities: Vec<SnapshotEntity> = s
            .iter()
            .map(|(entity, payload)| SnapshotEntity {
                entity: entity.clone(),
                payload: payload.clone(),
            })
            .collect();
        entities.sort_by(|a, b| a.entity.as_str().cmp(b.entity.as_str()));
        entities
    }

    fn apply_baseline(&self, entities: Vec<SnapshotEntity>) {
        let mut s = self.state.lock().unwrap();
        s.clear();
        for e in entities {
            s.insert(e.entity, e.payload);
        }
    }

    fn apply_event(&self, e: ResolvedEvent) {
        let mut s = self.state.lock().unwrap();
        match e.event_type.as_str() {
            "deleted" => {
                s.remove(&e.entity);
            }
            "created" => {
                s.insert(e.entity, e.payload);
            }
            // A modify only touches an existing entity: one arriving after a
            // delete must not resurrect it. Keeps deletes sticky and the fold
            // deterministic.
            _ => {
                if let Some(obj) = s.get_mut(&e.entity) {
                    merge(obj, &e.payload);
                }
            }
        }
    }
}

// -------------------------------------------------------------------------
// Simulated client: engine + reducer + download-then-upload sync loop.
// -------------------------------------------------------------------------

pub struct SimClient {
    pub name: &'static str,
    reducer: MapReducer,
    engine: EventSyncEngine,
}

impl SimClient {
    pub fn new(name: &'static str, schema: SchemaRegistry, config: BaselineConfig) -> Self {
        let reducer = MapReducer::new();
        let engine = EventSyncEngine::new(
            "containers",
            Box::new(reducer.clone()),
            schema,
            Box::new(MemoryMetadataStore::new()),
        )
        .with_baseline_config(config);
        Self {
            name,
            reducer,
            engine,
        }
    }

    pub fn create(&self, fields: Value) -> Guid {
        let id = Guid::random();
        self.reducer.local_create(id.clone(), fields.clone());
        self.engine
            .record_local_event("created", id.clone(), fields)
            .unwrap();
        id
    }

    pub fn modify(&self, id: &Guid, field: &str, value: Value) {
        let payload = json!({ field: value });
        self.reducer.local_merge(id, &payload);
        self.engine
            .record_local_event(&format!("modify-{field}"), id.clone(), payload)
            .unwrap();
    }

    pub fn delete(&self, id: &Guid) {
        self.reducer.local_remove(id);
        self.engine
            .record_local_event("deleted", id.clone(), json!({}))
            .unwrap();
    }

    /// One download-then-upload round trip. Returns whether anything moved.
    pub fn sync(&self, server: &mut FakeServer) -> bool {
        let mut telem = telemetry::Engine::new("containers");
        let now = server.timestamp();
        let newer = self
            .engine
            .get_collection_request(now)
            .unwrap()
            .and_then(|r| r.newer);
        let incoming = server.fetch(newer);
        let downloaded = incoming.len();
        self.engine.stage_incoming(incoming, &mut telem).unwrap();
        let outgoing = self.engine.apply(now, &mut telem).unwrap();
        let uploaded = outgoing.len();
        if uploaded > 0 {
            let (ts, ids) = server.upload(outgoing);
            self.engine.set_uploaded(ts, ids).unwrap();
        }
        downloaded > 0 || uploaded > 0
    }

    pub fn state(&self) -> HashMap<Guid, Value> {
        self.reducer.state()
    }

    /// State projected to `{entity -> name}`, for comparing clients with
    /// different schema shapes.
    pub fn names(&self) -> BTreeMap<String, String> {
        self.state()
            .into_iter()
            .filter_map(|(id, payload)| {
                payload
                    .get("name")
                    .and_then(Value::as_str)
                    .map(|n| (id.to_string(), n.to_string()))
            })
            .collect()
    }
}

// -------------------------------------------------------------------------
// Drivers.
// -------------------------------------------------------------------------

/// Sync every client (in order) repeatedly until a full round changes nothing.
pub fn sync_to_quiescence(clients: &[&SimClient], server: &mut FakeServer) {
    for _ in 0..(clients.len() * 4 + 8) {
        let mut active = false;
        for c in clients {
            if c.sync(server) {
                active = true;
            }
        }
        if !active {
            return;
        }
    }
    panic!("clients did not reach quiescence");
}

pub fn assert_converged(clients: &[&SimClient]) {
    let expected = clients[0].state();
    for c in &clients[1..] {
        assert_eq!(
            c.state(),
            expected,
            "client `{}` diverged from `{}`",
            c.name,
            clients[0].name
        );
    }
}

pub fn no_baselines() -> BaselineConfig {
    BaselineConfig {
        events_per_baseline: None,
        baseline_timeout_ms: None,
        ..BaselineConfig::default()
    }
}

pub fn schema_v1() -> SchemaRegistry {
    SchemaRegistry::new(1)
}

/// Schema 2 adds an `icon` field; up defaults it, down drops it.
pub fn schema_v2() -> SchemaRegistry {
    let mut reg = SchemaRegistry::new(1);
    reg.register_migration(
        2,
        Box::new(|_ty, mut v| {
            if let Some(obj) = v.as_object_mut() {
                obj.entry("icon").or_insert(json!("default"));
            }
            v
        }),
        Box::new(|_ty, mut v| {
            if let Some(obj) = v.as_object_mut() {
                obj.remove("icon");
            }
            v
        }),
    );
    reg
}

/// Schema 3 adds a `sort` field on top of schema 2.
pub fn schema_v3() -> SchemaRegistry {
    let mut reg = schema_v2();
    reg.register_migration(
        3,
        Box::new(|_ty, mut v| {
            if let Some(obj) = v.as_object_mut() {
                obj.entry("sort").or_insert(json!(0));
            }
            v
        }),
        Box::new(|_ty, mut v| {
            if let Some(obj) = v.as_object_mut() {
                obj.remove("sort");
            }
            v
        }),
    );
    reg
}
