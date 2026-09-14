/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! A runnable containers consumer for the event sync engine.
//!
//! Run with: `cargo run -p sync_engine --example containers`

use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use sync15::bso::{IncomingBso, OutgoingBso};
use sync15::engine::SyncEngine;
use sync15::{telemetry, Guid, ServerTimestamp};
use sync_engine::{
    BaselineConfig, EventReducer, EventSyncEngine, MemoryMetadataStore, ResolvedEvent,
    SchemaRegistry, BASELINE_EVENT_TYPE,
};

#[derive(Default)]
struct FakeServer {
    clock: i64,
    records: HashMap<Guid, (OutgoingBso, ServerTimestamp)>,
}

impl FakeServer {
    fn timestamp(&self) -> ServerTimestamp {
        ServerTimestamp::from_millis(self.clock)
    }

    fn fetch(&self, newer: Option<ServerTimestamp>) -> Vec<IncomingBso> {
        let since = newer.map_or(0, |t| t.as_millis());
        self.records
            .values()
            .filter(|(_, ts)| ts.as_millis() > since)
            .map(|(bso, ts)| bso.to_test_incoming_ts(*ts))
            .collect()
    }

    fn upload(&mut self, bsos: Vec<OutgoingBso>) -> (ServerTimestamp, Vec<Guid>) {
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

    fn baseline_count(&self) -> usize {
        self.records
            .values()
            .filter(|(bso, _)| {
                serde_json::from_str::<Value>(&bso.payload)
                    .ok()
                    .map(|v| {
                        v.get("type").and_then(Value::as_str) == Some(BASELINE_EVENT_TYPE)
                            || v.get("data").is_some()
                    })
                    .unwrap_or(false)
            })
            .count()
    }
}

#[derive(Clone, Default)]
struct ContainerStore {
    state: Arc<Mutex<HashMap<Guid, Value>>>,
}

fn merge(target: &mut Value, src: &Value) {
    if let (Some(t), Some(s)) = (target.as_object_mut(), src.as_object()) {
        for (k, v) in s {
            t.insert(k.clone(), v.clone());
        }
    }
}

impl ContainerStore {
    fn dump(&self) -> BTreeMap<String, Value> {
        self.state
            .lock()
            .unwrap()
            .iter()
            .map(|(id, v)| (id.to_string(), v.clone()))
            .collect()
    }

    fn names(&self) -> BTreeMap<String, String> {
        self.state
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(id, v)| {
                v.get("name")
                    .and_then(Value::as_str)
                    .map(|n| (id.to_string(), n.to_string()))
            })
            .collect()
    }
}

impl EventReducer for ContainerStore {
    fn baseline(&self) -> Value {
        let s = self.state.lock().unwrap();
        let mut out: Vec<Value> = s
            .iter()
            .map(|(entity, payload)| json!({ "entity": entity, "payload": payload }))
            .collect();
        // Deterministic order, so the same state produces the same bytes.
        out.sort_by(|a, b| a["entity"].as_str().cmp(&b["entity"].as_str()));
        Value::Array(out)
    }

    fn apply_baseline(&self, payload: Value) {
        let mut s = self.state.lock().unwrap();
        s.clear();
        if let Value::Array(items) = payload {
            for item in items {
                if let Some(id) = item["entity"].as_str() {
                    s.insert(Guid::new(id), item["payload"].clone());
                }
            }
        }
    }

    fn apply_event(&self, e: ResolvedEvent) {
        let mut s = self.state.lock().unwrap();
        match e.event_type.as_str() {
            "created" => {
                s.insert(e.entity, e.payload);
            }
            "deleted" => {
                s.remove(&e.entity);
            }
            _ => {
                if let Some(obj) = s.get_mut(&e.entity) {
                    merge(obj, &e.payload);
                }
            }
        }
    }
}

fn container_schema_v1() -> SchemaRegistry {
    SchemaRegistry::new(1)
}

fn container_schema_v2() -> SchemaRegistry {
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

struct Client {
    store: ContainerStore,
    engine: EventSyncEngine,
}

impl Client {
    fn new(schema: SchemaRegistry, config: BaselineConfig) -> Self {
        let store = ContainerStore::default();
        let engine = EventSyncEngine::new(
            "containers",
            Box::new(store.clone()),
            schema,
            Box::new(MemoryMetadataStore::new()),
        )
        .with_baseline_config(config);
        Self { store, engine }
    }

    fn create(&self, fields: Value) -> Guid {
        let id = Guid::random();
        self.store
            .state
            .lock()
            .unwrap()
            .insert(id.clone(), fields.clone());
        self.engine
            .record_local_event("created", id.clone(), fields)
            .unwrap();
        id
    }

    fn modify(&self, id: &Guid, field: &str, value: Value) {
        let payload = json!({ field: value });
        merge(
            self.store
                .state
                .lock()
                .unwrap()
                .entry(id.clone())
                .or_insert(json!({})),
            &payload,
        );
        self.engine
            .record_local_event(&format!("modify-{field}"), id.clone(), payload)
            .unwrap();
    }

    fn delete(&self, id: &Guid) {
        self.store.state.lock().unwrap().remove(id);
        self.engine
            .record_local_event("deleted", id.clone(), json!({}))
            .unwrap();
    }

    fn sync(&self, server: &mut FakeServer) -> bool {
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
}

fn sync_to_quiescence(clients: &[&Client], server: &mut FakeServer) {
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

fn main() {
    let off = || BaselineConfig {
        events_per_baseline: None,
        baseline_timeout_ms: None,
        ..BaselineConfig::default()
    };

    {
        println!("== delete is sticky against concurrent modify ==");
        let mut server = FakeServer::default();
        let a = Client::new(container_schema_v1(), off());
        let b = Client::new(container_schema_v1(), off());

        let id = a.create(json!({ "name": "doomed", "color": "blue" }));
        sync_to_quiescence(&[&a, &b], &mut server);

        a.delete(&id);
        b.modify(&id, "name", json!("too-late"));
        sync_to_quiescence(&[&a, &b], &mut server);

        assert_eq!(a.store.dump(), b.store.dump());
        assert!(a.store.dump().is_empty(), "the delete must win");
        println!("both clients dropped the container; a late modify can't resurrect it\n");
    }

    {
        println!("== modify-name resolves to one winner (per-field gate) ==");
        let mut server = FakeServer::default();
        let a = Client::new(container_schema_v1(), off());
        let b = Client::new(container_schema_v1(), off());

        let id = a.create(json!({ "name": "orig", "color": "blue" }));
        sync_to_quiescence(&[&a, &b], &mut server);

        a.modify(&id, "name", json!("from-A"));
        b.modify(&id, "name", json!("from-B"));
        b.modify(&id, "color", json!("green"));
        sync_to_quiescence(&[&a, &b], &mut server);

        assert_eq!(a.store.dump(), b.store.dump());
        let name = a.store.names()[&id.to_string()].clone();
        assert!(
            name == "from-A" || name == "from-B",
            "unexpected name {name}"
        );
        assert_eq!(a.store.dump()[&id.to_string()]["color"], json!("green"));
        println!("name = {name:?} (one winner), color = \"green\" (survives alongside)\n");
    }

    {
        println!("== old and new schema clients interoperate ==");
        let mut server = FakeServer::default();
        let modern = Client::new(container_schema_v2(), off());
        let legacy = Client::new(container_schema_v1(), off());

        modern.create(json!({ "name": "modern", "color": "blue", "icon": "star" }));
        legacy.create(json!({ "name": "legacy", "color": "red" }));

        sync_to_quiescence(&[&modern, &legacy], &mut server);

        assert_eq!(modern.store.names(), legacy.store.names());
        let migrated = modern
            .store
            .dump()
            .into_values()
            .any(|v| v["name"] == json!("legacy") && v["icon"] == json!("default"));
        assert!(migrated, "v2 should migrate the v1 container up");
        println!("v2 raw: {:?}", modern.store.dump());
        println!("v1 raw: {:?}\n", legacy.store.dump());
    }

    {
        println!("== late joiner cold-starts from a baseline ==");
        let mut server = FakeServer::default();
        let a = Client::new(
            container_schema_v1(),
            BaselineConfig {
                events_per_baseline: Some(3),
                baseline_timeout_ms: None,
                ..BaselineConfig::default()
            },
        );

        a.create(json!({ "name": "one" }));
        a.create(json!({ "name": "two" }));
        a.create(json!({ "name": "three" }));
        sync_to_quiescence(&[&a], &mut server);
        assert!(server.baseline_count() >= 1, "A should write a baseline");

        let b = Client::new(container_schema_v1(), off());
        sync_to_quiescence(&[&a, &b], &mut server);

        assert_eq!(a.store.dump(), b.store.dump());
        assert_eq!(b.store.dump().len(), 3);
        println!(
            "server holds {} baseline record(s); late joiner reconstructed {} containers\n",
            server.baseline_count(),
            b.store.dump().len()
        );
    }

    println!("all container scenarios converged.");
}
