/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Tests for sync payload evolution.  If we see new fields on incoming sync records, we should
//! make sure to roundtrip them when we sync them back.

use crate::api::places_api::test::new_mem_api;
use crate::bookmark_sync::record::BookmarkItemRecord;
use crate::bookmark_sync::BookmarksSyncEngine;
use crate::storage::bookmarks::{update_bookmark_from_info, BookmarkUpdateInfo};
use crate::PlacesApi;
use serde_json::{json, Value};
use std::collections::HashMap;
use sync15::{
    bso::{IncomingBso, IncomingKind, OutgoingBso},
    engine::SyncEngine,
    telemetry, ServerTimestamp,
};

#[test]
fn test_roundtrip_fields() {
    // Test that we roundtrip unknown fields from remote records after making changes to them
    RoundtripTest {
        initial_remote_records: vec![
            remote_bookmark(guid('a'), "Dogs", vec![("device_type", "phone")]),
            remote_bookmark(guid('b'), "Cats", vec![]),
            remote_folder(
                guid('c'),
                "Animals",
                vec!["A", "B"],
                vec![("device_type", "phone"), ("color", "blue")],
            ),
            remote_separator(guid('d'), 2, vec![("device_type", "desktop")]),
            // Create the unfiled bookmarks folder, or else dogear will try to delete the query
            // rather than sync it (https://github.com/mozilla/application-services/pull/5438#discussion_r1143960257)
            remote_item(
                json!({
                    "id": "unfiled",
                    "title": "Unfiled Bookmarks",
                    "type": "folder",
                    "parentid": "places",
                    "parentName": "places",
                    "dateAdded": 0,
                    "children": vec![guid('e')],
                }),
                vec![],
            ),
            // Pretend like we can update query items to test payload evolution, even though we
            // don't actually support it in the API.
            remote_query(
                guid('e'),
                "dog search",
                "place:folder=123&excludeItems=1",
                vec![("device_type", "desktop")],
            ),
        ],
        local_updates: vec![
            title_change(guid('a'), "Doggies"),
            title_change(guid('b'), "Kitties"),
            title_change(guid('c'), "Cute Animals"),
            pos_change(guid('d'), 3),
            title_change(guid('e'), "Doggy search"),
        ],
        incoming_remote_records: vec![],
        outgoing_unknown_fields: vec![
            (guid('a'), vec![("device_type", "phone")]),
            (guid('b'), vec![]),
            (guid('c'), vec![("device_type", "phone"), ("color", "blue")]),
            (guid('d'), vec![("device_type", "desktop")]),
            (guid('e'), vec![("device_type", "desktop")]),
        ],
    }
    .run()
}

#[test]
fn test_new_unknown_fields() {
    // If we have new incoming remote records with new unknown fields, those should override the
    // ones from the mirror table
    RoundtripTest {
        initial_remote_records: vec![
            remote_bookmark(guid('a'), "Dogs", vec![("device_type", "phone")]),
            remote_bookmark(guid('b'), "Cats", vec![("device_type", "desktop")]),
        ],
        local_updates: vec![
            title_change(guid('a'), "Doggies"),
            title_change(guid('b'), "Kitties"),
        ],
        incoming_remote_records: vec![remote_bookmark(
            guid('a'),
            "Dogs",
            vec![("device_type", "mini-phone")],
        )],
        outgoing_unknown_fields: vec![
            (guid('a'), vec![("device_type", "mini-phone")]),
            (guid('b'), vec![("device_type", "desktop")]),
        ],
    }
    .run()
}

struct RoundtripTest {
    // Mirror records from a previous sync
    initial_remote_records: Vec<Value>,
    // Local updates to those mirror records before the current sync
    local_updates: Vec<BookmarkUpdateInfo>,
    // Incoming records for the current sync (records changed remotely since the previous sync)
    incoming_remote_records: Vec<Value>,
    // The unknown fields we expect to see on outgoing records
    outgoing_unknown_fields: Vec<(String, Vec<(&'static str, &'static str)>)>,
}

impl RoundtripTest {
    fn run(self) {
        let api = new_mem_api();
        let engine = BookmarksSyncEngine::new(api.get_sync_connection().unwrap()).unwrap();
        self.process_incoming_records(&engine, &self.initial_remote_records);
        self.make_local_updates(&api);
        let outgoing_items = self
            .process_incoming_records(&engine, &self.incoming_remote_records)
            .into_iter()
            // Parse outgoing items into BookmarkItemRecords
            .map(|i| {
                let content = i.to_test_incoming().into_content::<BookmarkItemRecord>();
                match content.kind {
                    IncomingKind::Content(record) => record,
                    IncomingKind::Tombstone => {
                        panic!("Unexpected tombstone in incoming record: {i:?}")
                    }
                    IncomingKind::Malformed => panic!("Malformed JSON in incoming record: {i:?}"),
                }
            })
            // Filter out global bookmark items
            .filter(|i| {
                !matches!(
                    i.record_id().as_guid().as_str(),
                    "menu________" | "toolbar_____" | "unfiled_____" | "mobile______"
                )
            })
            .collect::<Vec<BookmarkItemRecord>>();
        // Check that the outgoing item GUIDs matches what we expect
        let mut correct_outgoing_unknown_fields: HashMap<String, Vec<(String, String)>> = self
            .outgoing_unknown_fields
            .into_iter()
            .map(|(key, fields)| {
                (
                    key,
                    fields
                        .iter()
                        .map(|(s1, s2)| (s1.to_string(), s2.to_string()))
                        .collect(),
                )
            })
            .collect();
        let mut correct_outgoing_keys = correct_outgoing_unknown_fields
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        // For each outgoing item, check that the unknown fields match what we expect
        let mut outgoing_keys = outgoing_items
            .iter()
            .map(|i| i.record_id().as_guid().to_string())
            .collect::<Vec<_>>();
        correct_outgoing_keys.sort();
        outgoing_keys.sort();
        assert_eq!(outgoing_keys, correct_outgoing_keys);
        for bookmark_item in outgoing_items {
            let guid = bookmark_item.record_id().as_guid().to_string();
            let mut unknown_fields: Vec<(String, String)> = bookmark_item
                .unknown_fields()
                .iter()
                .map(|(key, value)| (key.clone(), value.as_str().unwrap().to_string()))
                .collect();
            let correct_unknown_fields = correct_outgoing_unknown_fields.get_mut(&guid).unwrap();
            unknown_fields.sort();
            correct_unknown_fields.sort();
            assert_eq!(
                &unknown_fields, correct_unknown_fields,
                "Unexpected unknown fields for record with guid: {guid}"
            );
        }
    }

    fn process_incoming_records(
        &self,
        engine: &BookmarksSyncEngine,
        records: &[Value],
    ) -> Vec<OutgoingBso> {
        let changes = records.iter().map(IncomingBso::from_test_content).collect();

        let mut telem = telemetry::Engine::new("bookmarks");
        engine
            .stage_incoming(changes, &mut telem)
            .expect("Should stage incoming records");
        engine.apply(now(), &mut telem).expect("should apply")
    }

    fn make_local_updates(&self, api: &PlacesApi) {
        let conn = api
            .open_connection(crate::ConnectionType::ReadWrite)
            .unwrap();
        for update in &self.local_updates {
            update_bookmark_from_info(&conn, update.clone()).unwrap();
        }
    }
}

fn remote_bookmark(guid: String, title: &str, extra_fields: Vec<(&str, &str)>) -> Value {
    let uri = format!("http://example.com/{guid}");
    remote_item(
        json!({
            "id": guid,
            "type": "bookmark",
            "parentid": "menu",
            "parentName": "menu",
            "dateAdded": before().to_string(),
            "title": title,
            "bmkUri": uri
        }),
        extra_fields,
    )
}

fn remote_folder(
    guid: String,
    title: &str,
    children_ids: Vec<&str>,
    extra_fields: Vec<(&str, &str)>,
) -> Value {
    remote_item(
        json!({
            "id": guid,
            "title": title,
            "type": "folder",
            "parentid": "menu",
            "parentName": "menu",
            "dateAdded": before().to_string(),
            "children": children_ids,
        }),
        extra_fields,
    )
}

fn remote_separator(guid: String, pos: u32, extra_fields: Vec<(&str, &str)>) -> Value {
    remote_item(
        json!({
            "id": guid,
            "type": "separator",
            "parentid": "unfiled",
            "parentName": "Unfiled Bookmarks",
            "pos": pos,
        }),
        extra_fields,
    )
}

fn remote_query(guid: String, title: &str, url: &str, extra_fields: Vec<(&str, &str)>) -> Value {
    remote_item(
        json!({
            "id": guid,
            "type": "query",
            "parentid": "unfiled",
            "parentName": "Unfiled Bookmarks",
            "dateAdded": before().to_string(),
            "title": title,
            "bmkUri": url,
        }),
        extra_fields,
    )
}

fn remote_item(mut item: Value, extra_fields: Vec<(&str, &str)>) -> Value {
    let obj = item.as_object_mut().unwrap();
    for (key, val) in extra_fields {
        obj.insert(key.to_string(), val.to_string().into());
    }
    item
}

fn title_change(guid: String, new_title: &str) -> BookmarkUpdateInfo {
    BookmarkUpdateInfo {
        guid: guid.into(),
        title: Some(new_title.into()),
        url: None,
        parent_guid: None,
        position: None,
    }
}

fn pos_change(guid: String, new_pos: u32) -> BookmarkUpdateInfo {
    BookmarkUpdateInfo {
        guid: guid.into(),
        position: Some(new_pos),
        title: None,
        url: None,
        parent_guid: None,
    }
}

fn before() -> ServerTimestamp {
    ServerTimestamp::from_millis(1577750400000) // 2019-12-31
}

fn now() -> ServerTimestamp {
    ServerTimestamp::from_millis(1577836800000) // 2020-01-01
}

// Generate valid guids from a single character
fn guid(name: char) -> String {
    name.to_string().repeat(12)
}
