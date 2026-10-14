/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! A runnable bookmarks consumer for the event sync engine.
//!
//! Run with: `cargo run -p event_based_sync_engine --example bookmarks`

use event_based_sync_engine::{
    BaselineConfig, EventReducer, EventSyncEngine, MemoryMetadataStore, ResolvedEvent,
    SchemaRegistry,
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use sync15::bso::{IncomingBso, OutgoingBso};
use sync15::engine::SyncEngine;
use sync15::{telemetry, Guid, ServerTimestamp};

const MENU: &str = "menu________";
const TOOLBAR: &str = "toolbar_____";
const UNFILED: &str = "unfiled_____";
const MOBILE: &str = "mobile______";
const ROOTS: [&str; 4] = [MENU, TOOLBAR, UNFILED, MOBILE];

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
}

#[derive(Clone)]
struct BookmarkStore {
    state: Arc<Mutex<HashMap<Guid, Value>>>,
}

fn merge(target: &mut Value, src: &Value) {
    if let (Some(t), Some(s)) = (target.as_object_mut(), src.as_object()) {
        for (k, v) in s {
            t.insert(k.clone(), v.clone());
        }
    }
}

impl Default for BookmarkStore {
    fn default() -> Self {
        let mut state = HashMap::new();
        for root in ROOTS {
            state.insert(
                Guid::new(root),
                json!({ "type": "folder", "title": root, "children": [] }),
            );
        }
        Self {
            state: Arc::new(Mutex::new(state)),
        }
    }
}

impl BookmarkStore {
    fn dump(&self) -> BTreeMap<String, Value> {
        self.state
            .lock()
            .unwrap()
            .iter()
            .map(|(id, v)| (id.to_string(), v.clone()))
            .collect()
    }

    fn tree(&self) -> BTreeMap<String, Vec<String>> {
        let s = self.state.lock().unwrap();
        let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for id in s.keys() {
            if s.get(id).and_then(|v| v.get("type")).map(|t| t == "folder") == Some(true) {
                out.entry(id.to_string()).or_default();
            }
        }
        for (id, v) in s.iter() {
            if ROOTS.contains(&id.as_str()) {
                continue;
            }
            let parent = v.get("parentid").and_then(Value::as_str).unwrap_or(UNFILED);
            let parent = if s.contains_key(&Guid::new(parent)) {
                parent
            } else {
                UNFILED
            };
            out.entry(parent.to_string())
                .or_default()
                .push(id.to_string());
        }
        for children in out.values_mut() {
            children.sort();
        }
        out
    }

    fn titles(&self) -> BTreeMap<String, String> {
        self.state
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(id, v)| {
                v.get("title")
                    .and_then(Value::as_str)
                    .map(|t| (id.to_string(), t.to_string()))
            })
            .collect()
    }
}

impl EventReducer for BookmarkStore {
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

    fn wipe(&self) {
        self.state.lock().unwrap().clear();
    }
}

fn bookmark_schema_v1() -> SchemaRegistry {
    SchemaRegistry::new(1)
}

fn bookmark_schema_v2() -> SchemaRegistry {
    let mut reg = SchemaRegistry::new(1);
    reg.register_migration(
        2,
        Box::new(|_ty, mut v| {
            if let Some(obj) = v.as_object_mut() {
                obj.entry("tags").or_insert(json!([]));
            }
            v
        }),
        Box::new(|_ty, mut v| {
            if let Some(obj) = v.as_object_mut() {
                obj.remove("tags");
            }
            v
        }),
    );
    reg
}

struct Client {
    store: BookmarkStore,
    engine: EventSyncEngine,
}

impl Client {
    fn new(schema: SchemaRegistry, config: BaselineConfig) -> Self {
        let store = BookmarkStore::default();
        let engine = EventSyncEngine::new(
            "bookmarks",
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

    fn add_bookmark(&self, parent: &str, title: &str, url: &str, date_added: i64) -> Guid {
        self.create(json!({
            "type": "bookmark",
            "parentid": parent,
            "title": title,
            "bmkUri": url,
            "dateAdded": date_added,
        }))
    }

    fn add_folder(&self, parent: &str, title: &str, date_added: i64) -> Guid {
        self.create(json!({
            "type": "folder",
            "parentid": parent,
            "title": title,
            "dateAdded": date_added,
        }))
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

    fn move_to(&self, id: &Guid, new_parent: &str) {
        self.modify(id, "parentid", json!(new_parent));
    }

    fn delete(&self, id: &Guid) {
        self.store.state.lock().unwrap().remove(id);
        self.engine
            .record_local_event("deleted", id.clone(), json!({}))
            .unwrap();
    }

    fn sync(&self, server: &mut FakeServer) -> bool {
        let mut telem = telemetry::Engine::new("bookmarks");
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
    };

    {
        println!("== independent creates union across the tree ==");
        let mut server = FakeServer::default();
        let a = Client::new(bookmark_schema_v1(), off());
        let b = Client::new(bookmark_schema_v1(), off());

        let folder = a.add_folder(MENU, "Reading", 1);
        a.add_bookmark(folder.as_ref(), "Rust", "https://rust-lang.org", 2);
        b.add_bookmark(UNFILED, "Mozilla", "https://mozilla.org", 3);

        sync_to_quiescence(&[&a, &b], &mut server);
        assert_eq!(a.store.dump(), b.store.dump());
        assert_eq!(a.store.tree(), b.store.tree());
        assert_eq!(a.store.dump().len(), 7);
        assert_eq!(a.store.tree()[MENU], vec![folder.to_string()]);
        assert_eq!(a.store.tree()[&folder.to_string()].len(), 1);
        println!("titles: {:?}\n", a.store.titles());
    }

    {
        println!("== concurrent move resolves to one parent (per-field gate) ==");
        let mut server = FakeServer::default();
        let a = Client::new(bookmark_schema_v1(), off());
        let b = Client::new(bookmark_schema_v1(), off());

        let id = a.add_bookmark(MENU, "wandering", "https://example.org", 1);
        sync_to_quiescence(&[&a, &b], &mut server);

        a.move_to(&id, TOOLBAR);
        b.move_to(&id, MOBILE);
        b.modify(&id, "keyword", json!("wander"));
        sync_to_quiescence(&[&a, &b], &mut server);

        assert_eq!(a.store.dump(), b.store.dump());
        assert_eq!(a.store.tree(), b.store.tree());
        let parent = a.store.dump()[&id.to_string()]["parentid"].clone();
        assert!(
            parent == json!(TOOLBAR) || parent == json!(MOBILE),
            "one parent wins"
        );
        assert_eq!(a.store.dump()[&id.to_string()]["keyword"], json!("wander"));
        println!("lives under {parent} (one winner), keyword survives alongside\n");
    }

    {
        println!("== folder delete is sticky; moved-in child becomes an orphan ==");
        let mut server = FakeServer::default();
        let a = Client::new(bookmark_schema_v1(), off());
        let b = Client::new(bookmark_schema_v1(), off());

        let folder = a.add_folder(MENU, "doomed", 1);
        let mark = a.add_bookmark(MENU, "survivor", "https://keep.me", 2);
        sync_to_quiescence(&[&a, &b], &mut server);

        a.delete(&folder);
        b.move_to(&mark, folder.as_ref());
        sync_to_quiescence(&[&a, &b], &mut server);

        assert_eq!(a.store.dump(), b.store.dump());
        assert_eq!(a.store.tree(), b.store.tree());
        assert!(
            !a.store.dump().contains_key(&folder.to_string()),
            "folder stays deleted"
        );
        assert!(a.store.dump().contains_key(&mark.to_string()));
        assert!(a.store.tree()[UNFILED].contains(&mark.to_string()));
        println!("folder gone, bookmark kept and reparented to unfiled\n");
    }

    {
        println!("== old and new schema clients interoperate ==");
        let mut server = FakeServer::default();
        let modern = Client::new(bookmark_schema_v2(), off());
        let legacy = Client::new(bookmark_schema_v1(), off());

        modern.create(json!({
            "type": "bookmark", "parentid": UNFILED,
            "title": "tagged", "bmkUri": "https://a", "dateAdded": 1, "tags": ["rust"],
        }));
        legacy.add_bookmark(UNFILED, "plain", "https://b", 2);

        sync_to_quiescence(&[&modern, &legacy], &mut server);

        assert_eq!(modern.store.dump().len(), legacy.store.dump().len());
        assert_eq!(modern.store.titles(), legacy.store.titles());
        assert_eq!(modern.store.tree(), legacy.store.tree());
        let migrated = modern
            .store
            .dump()
            .into_values()
            .any(|v| v["title"] == json!("plain") && v["tags"] == json!([]));
        assert!(migrated, "v2 should migrate the v1 bookmark up");
        println!("shared titles: {:?}\n", modern.store.titles());
    }

    println!("all bookmark scenarios converged.");
}
