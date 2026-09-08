/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Baseline scenarios: a late joiner cold-starts from a baseline instead of the
//! full stream, and an oversized baseline splits into parts that reassemble.

mod support;

use serde_json::json;
use support::{
    assert_converged, no_baselines, schema_v1, sync_to_quiescence, FakeServer, SimClient,
};
use sync_engine::BaselineConfig;

#[test]
fn late_joiner_cold_starts_from_a_baseline() {
    let mut server = FakeServer::new();
    // Low threshold so a baseline is written after a few events.
    let config = BaselineConfig {
        events_per_baseline: Some(3),
        baseline_timeout_ms: None,
        ..BaselineConfig::default()
    };
    let a = SimClient::new("A", schema_v1(), config);

    a.create(json!({ "name": "one" }));
    a.create(json!({ "name": "two" }));
    a.create(json!({ "name": "three" }));

    // A syncs alone: uploads events, then writes a baseline.
    sync_to_quiescence(&[&a], &mut server);
    assert!(
        server.baseline_count() >= 1,
        "A should have written a baseline"
    );

    // A new client joins empty and reconstructs from the baseline plus newer
    // events, not the full stream.
    let b = SimClient::new("B", schema_v1(), no_baselines());
    sync_to_quiescence(&[&a, &b], &mut server);

    assert_converged(&[&a, &b]);
    assert_eq!(b.state().len(), 3);
}

#[test]
fn split_baseline_reassembles_on_a_late_joiner() {
    let mut server = FakeServer::new();
    // Tiny record budget forces about one entity per part. Low threshold makes
    // the baseline land quickly.
    let config = || BaselineConfig {
        events_per_baseline: Some(3),
        baseline_timeout_ms: None,
        max_record_payload_bytes: 200,
    };
    let a = SimClient::new("A", schema_v1(), config());

    // More than ten containers, so a split produces `_10`, `_11`, ... and the
    // late joiner must reassemble by numeric part, not record-id order.
    for i in 0..12 {
        a.create(json!({ "name": format!("c{i}") }));
    }
    sync_to_quiescence(&[&a], &mut server);
    assert!(
        server.baseline_count() >= 12,
        "the baseline should have been split into >=12 parts, got {}",
        server.baseline_count()
    );

    // A fresh client cold-starts from the split baseline and rebuilds all 12.
    let b = SimClient::new("B", schema_v1(), no_baselines());
    sync_to_quiescence(&[&a, &b], &mut server);

    assert_converged(&[&a, &b]);
    assert_eq!(b.state().len(), 12);
}
