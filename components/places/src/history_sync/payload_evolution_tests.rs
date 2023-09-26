/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Tests for sync payload evolution.  These are in a separate file because they test the
//! functionality from several modules.

use crate::{
    api::places_api::{test::new_mem_api, PlacesApi},
    history_sync::{record::HistoryRecord, HistorySyncEngine},
    observation::VisitObservation,
    storage::history::apply_observation,
    types::{UnknownFields, VisitType},
};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use sync15::{
    bso::{IncomingBso, IncomingKind, OutgoingBso},
    engine::SyncEngine,
    telemetry, ServerTimestamp,
};

// Macro to quickly make `UnknownFields` values.  It's like `json!`, but avoids the need to append
// `as_object().unwrap()` to each call.
macro_rules! unknown_fields(
   {
       {
           $($key:literal : $value: expr),* $(,)?
       }
   } => {
       serde_json::Map::from_iter([
           $(
               (String::from($key), serde_json::json!($value)),
           )*
       ])
   }
);

#[test]
fn test_history_records_roundtrip_fields() {
    // Test that we roundtrip unknown fields from remote history records after making changes to them
    RoundtripTest {
        initial_remote_records: vec![
            json!({
                "id": "a___________",
                "title": "dogs",
                "histUri": "https://dogs.com/",
                "visits": [],
                // Simulate 2 new features that nobody is asking for:
                //   - themes for specific URLs
                //   - prank the user when they try to open a URL
                "theme": "solarized",
                "prank": "rick-roll",
                "visits": [
                    {
                        "date": timestamp(1),
                        "type": 1,
                    },
                ]
            }),
            json!({
                "id": "b___________",
                "title": "cats",
                "histUri": "https://cats.com/",
                "visits": [
                    {
                        "date": timestamp(2),
                        "type": 1,
                    },
                    {
                        "date": timestamp(3),
                        "type": 1,
                    },
                ],
            }),
        ],
        local_visits: vec![
            visit("https://dogs.com/", timestamp(10)),
            visit("https://cats.com/", timestamp(11)),
        ],
        incoming_remote_records: vec![],
        outgoing_unknown_fields: vec![
            OutgoingRecordUnknownFields {
                guid: "a___________".into(),
                fields: unknown_fields!({
                    "theme": "solarized",
                    "prank": "rick-roll",
                }),
                visits_fields: vec![
                    (timestamp(1), unknown_fields!({})),
                    (timestamp(10), unknown_fields!({})),
                ],
            },
            OutgoingRecordUnknownFields {
                guid: "b___________".into(),
                fields: unknown_fields!({}),
                visits_fields: vec![
                    (timestamp(2), unknown_fields!({})),
                    (timestamp(3), unknown_fields!({})),
                    (timestamp(11), unknown_fields!({})),
                ],
            },
        ],
    }
    .run()
}

#[test]
fn test_history_records_new_unknown_field_values() {
    // If there are incoming history records with new unknown fields, we should use those rather than our
    // stored unknown fields
    RoundtripTest {
        initial_remote_records: vec![json!({
            "id": "a___________",
            "title": "dogs",
            "histUri": "https://dogs.com/",
            "visits": [],
            "theme": "solarized",
            "prank": "rick-roll",
            "visits": [
                {
                    "date": timestamp(1),
                    "type": 1,
                },
            ]
        })],
        local_visits: vec![visit("https://dogs.com/", timestamp(10))],
        incoming_remote_records: vec![json!({
            "id": "a___________",
            "title": "dogs",
            "histUri": "https://dogs.com/",
            "visits": [],
            "theme": "nord",
            // Note: no prank present, so there shouldn't be one in the outgoing unknown_fields
            "visits": [
                {
                    "date": timestamp(1),
                    "type": 1,
                },
                {
                    "date": timestamp(2),
                    "type": 1,
                },
            ]
        })],
        outgoing_unknown_fields: vec![OutgoingRecordUnknownFields {
            guid: "a___________".into(),
            fields: unknown_fields!({
                "theme": "nord",
            }),
            visits_fields: vec![
                (timestamp(1), unknown_fields!({})),
                (timestamp(2), unknown_fields!({})),
                (timestamp(10), unknown_fields!({})),
            ],
        }],
    }
    .run()
}

#[test]
fn test_history_visits_roundtrip_fields() {
    // Test that we roundtrip unknown fields from remote visits after making changes to them
    RoundtripTest {
        initial_remote_records: vec![json!({
            "id": "a___________",
            "title": "dogs",
            "histUri": "https://dogs.com/",
            "visits": [],
            "visits": [
                {
                    "date": timestamp(1),
                    "type": 1,
                    // Some more fake new features for the tests:
                    //   - What emotions did the page evoke?
                    //   - How much fake news was listed?
                    "emotion": "joy",
                    "fake-news-amount": "some",
                },
                {
                    "date": timestamp(2),
                    "type": 1,
                    "emotion": "anger",
                },
            ]
        })],
        local_visits: vec![visit("https://dogs.com/", timestamp(10))],
        incoming_remote_records: vec![],
        outgoing_unknown_fields: vec![OutgoingRecordUnknownFields {
            guid: "a___________".into(),
            fields: unknown_fields!({}),
            visits_fields: vec![
                (
                    timestamp(1),
                    unknown_fields!({
                        "emotion": "joy",
                        "fake-news-amount": "some",
                    }),
                ),
                (
                    timestamp(2),
                    unknown_fields!({
                        "emotion": "anger",
                    }),
                ),
                (timestamp(10), unknown_fields!({})),
            ],
        }],
    }
    .run()
}

#[test]
fn test_history_records_guid_mismatch() {
    // Test a remote record with a different GUID that a local record
    RoundtripTest {
        initial_remote_records: vec![],
        local_visits: vec![visit("https://dogs.com/", timestamp(1))],
        incoming_remote_records: vec![json!({
            "id": "a___________",
            "title": "dogs",
            "histUri": "https://dogs.com/",
            "theme": "solarized",
            "visits": [],
            "visits": [
                {
                    "date": timestamp(2),
                    "type": 1,
                    "emotion": "joy",
                },
            ]
        })],
        outgoing_unknown_fields: vec![OutgoingRecordUnknownFields {
            // We should use the remote GUID for unknown fields
            guid: "a___________".into(),
            fields: unknown_fields!({
                "theme": "solarized",
            }),
            visits_fields: vec![
                (timestamp(1), unknown_fields!({})),
                (
                    timestamp(2),
                    unknown_fields!({
                        "emotion": "joy",
                    }),
                ),
            ],
        }],
    }
    .run()
}

#[test]
fn test_history_record_dupes() {
    // Test the weird corner case 2 remote record with a different GUID, but the same URL.  In this case, we should:
    //  - Use the GUID from the first record
    //  - Use the unknown fields from the second record (odd, but consistent with how we handle
    //    `title`)
    //  - Merge the visit lists together, keeping the unknown fields from both
    RoundtripTest {
        initial_remote_records: vec![],
        local_visits: vec![visit("https://dogs.com/", timestamp(1))],
        incoming_remote_records: vec![
            json!({
                "id": "a___________",
                "title": "dogs",
                "histUri": "https://dogs.com/",
                "theme": "solarized",
                "visits": [],
                "visits": [
                    {
                        "date": timestamp(2),
                        "type": 1,
                        "emotion": "joy",
                    },
                ]
            }),
            json!({
                "id": "b___________",
                "title": "dogs",
                "histUri": "https://dogs.com/",
                "theme": "nord",
                "visits": [],
                "visits": [
                    {
                        "date": timestamp(3),
                        "type": 1,
                        "emotion": "anger",
                    },
                ]
            }),
        ],
        outgoing_unknown_fields: vec![OutgoingRecordUnknownFields {
            // We should use the GUID from the first incoming record
            guid: "a___________".into(),
            // Also take the record-level unknown_fields from the first incoming record
            fields: unknown_fields!({
                "theme": "nord",
            }),
            // The visits list should be merged together, and have unknown fields from both records
            visits_fields: vec![
                (timestamp(1), unknown_fields!({})),
                (
                    timestamp(2),
                    unknown_fields!({
                        "emotion": "joy",
                    }),
                ),
                (
                    timestamp(3),
                    unknown_fields!({
                        "emotion": "anger",
                    }),
                ),
            ],
        }],
    }
    .run()
}

// Note: we purposely don't support updating the unknown for existing visits.  Visits record an
// event at some moment in time, so it doesn't really make sense for clients to go back later and
// change the data.

struct RoundtripTest {
    // Mirror records from a previous sync
    initial_remote_records: Vec<Value>,
    // Local visits that happened after the previous sync
    local_visits: Vec<VisitObservation>,
    // Incoming records for the current sync (records changed remotely since the previous sync)
    incoming_remote_records: Vec<Value>,
    // The unknown fields we expect to see on outgoing records
    outgoing_unknown_fields: Vec<OutgoingRecordUnknownFields>,
}

impl RoundtripTest {
    fn run(self) {
        let api = new_mem_api();
        let engine = HistorySyncEngine::new(api.get_sync_connection().unwrap()).unwrap();
        self.process_incoming_records(&engine, &self.initial_remote_records);
        self.make_local_updates(&api);
        let outgoing_unknown_fields = self
            .process_incoming_records(&engine, &self.incoming_remote_records)
            .into_iter()
            // Parse outgoing items into HistoryRecord instances
            .map(|i| {
                let content = i.to_test_incoming().into_content::<HistoryRecord>();
                match content.kind {
                    IncomingKind::Content(record) => record,
                    IncomingKind::Tombstone => {
                        panic!("Unexpected tombstone in incoming record: {i:?}")
                    }
                    IncomingKind::Malformed => panic!("Malformed JSON in incoming record: {i:?}"),
                }
            })
            // Convert HistoryRecords to OutgoingRecordUnknownFields
            .map(|record| OutgoingRecordUnknownFields {
                guid: record.id.as_str().to_string(),
                fields: record.unknown_fields,
                visits_fields: record
                    .visits
                    .into_iter()
                    .map(|v| (v.date.0 as i64, v.unknown_fields))
                    .collect(),
            })
            .collect();
        assert_outgoing_unknown_fields_eq(self.outgoing_unknown_fields, outgoing_unknown_fields);
    }

    fn process_incoming_records(
        &self,
        engine: &HistorySyncEngine,
        records: &[Value],
    ) -> Vec<OutgoingBso> {
        let changes = records.iter().map(IncomingBso::from_test_content).collect();

        let mut telem = telemetry::Engine::new("history");
        engine
            .stage_incoming(changes, &mut telemetry::Engine::new("history"))
            .expect("Should stageapply incoming and stage outgoing records");
        let timestamp = ServerTimestamp::from_millis(timestamp(1000));
        engine.apply(timestamp, &mut telem).expect("should apply")
    }

    fn make_local_updates(&self, api: &PlacesApi) {
        let conn = api
            .open_connection(crate::ConnectionType::ReadWrite)
            .unwrap();
        for visit_ob in self.local_visits.clone() {
            apply_observation(&conn, visit_ob).unwrap();
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct OutgoingRecordUnknownFields {
    guid: String,
    fields: UnknownFields,
    visits_fields: Vec<(i64, UnknownFields)>,
}

fn assert_outgoing_unknown_fields_eq(
    expected_list: Vec<OutgoingRecordUnknownFields>,
    actual_list: Vec<OutgoingRecordUnknownFields>,
) {
    let expected_map: HashMap<_, _> = expected_list
        .into_iter()
        .map(|uf| (uf.guid.clone(), uf))
        .collect();
    let actual_map: HashMap<_, _> = actual_list
        .into_iter()
        .map(|uf| (uf.guid.clone(), uf))
        .collect();
    assert_eq!(
        HashSet::<&String>::from_iter(expected_map.keys()),
        HashSet::<&String>::from_iter(actual_map.keys())
    );
    for (key, mut expected) in expected_map {
        let mut actual = actual_map[&key].clone();
        assert_eq!(expected.fields, actual.fields);
        expected
            .visits_fields
            .sort_by(|(ts0, _), (ts1, _)| ts0.cmp(ts1));
        actual
            .visits_fields
            .sort_by(|(ts0, _), (ts1, _)| ts0.cmp(ts1));
        assert_eq!(expected.visits_fields, actual.visits_fields);
    }
}

fn visit(url: &'static str, timestamp: i64) -> VisitObservation {
    let timestamp = (timestamp / 1000) as u64;
    VisitObservation::new(url.parse().unwrap())
        .with_visit_type(VisitType::Link)
        .with_at(Some(timestamp.into()))
}

fn timestamp(amount: i64) -> i64 {
    let start = 1578000000000000; // round number near 2020-01-01
    start + amount * 1000
}
