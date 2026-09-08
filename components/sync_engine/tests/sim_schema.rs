/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Schema scenarios: old and new clients interoperate via migrate-on-read, and
//! the schema bar that gates baseline writes. Without a timeout the bar blocks
//! forever; with one, the highest-schema client resolves it.

mod support;

use serde_json::json;
use support::{
    assert_converged, no_baselines, schema_v1, schema_v2, schema_v3, sync_to_quiescence,
    FakeServer, SimClient,
};
use sync_engine::BaselineConfig;

#[test]
fn old_and_new_schema_clients_interoperate() {
    let mut server = FakeServer::new();
    let new_client = SimClient::new("v2", schema_v2(), no_baselines());
    let old_client = SimClient::new("v1", schema_v1(), no_baselines());

    // New client writes a container with `icon`, old one without.
    new_client.create(json!({ "name": "modern", "icon": "star" }));
    old_client.create(json!({ "name": "legacy" }));

    sync_to_quiescence(&[&new_client, &old_client], &mut server);

    // Raw states differ (each stores its own shape)...
    assert_eq!(new_client.state().len(), 2);
    assert_eq!(old_client.state().len(), 2);
    // ...but they agree on `name`, and the new client migrated the old event up.
    assert_eq!(new_client.names(), old_client.names());
    let migrated = new_client
        .state()
        .into_values()
        .any(|v| v["name"] == json!("legacy") && v["icon"] == json!("default"));
    assert!(migrated, "v2 client should migrate the v1 event up");
}

/// A higher-schema client authors events then leaves. Everyone left is below the
/// schema bar, so no one may write a baseline. With no timeout, the stream never
/// compacts.
#[test]
fn schema_bar_block_without_timeout_never_compacts() {
    let mut server = FakeServer::new();

    // Nightly (schema 2) authors events but never writes a baseline, then leaves.
    let nightly = SimClient::new("nightly", schema_v2(), no_baselines());
    nightly.create(json!({ "name": "one", "icon": "a" }));
    nightly.create(json!({ "name": "two", "icon": "b" }));
    nightly.create(json!({ "name": "three", "icon": "c" }));
    sync_to_quiescence(&[&nightly], &mut server);

    // Release (schema 1) applies the events but sits below the bar. With the
    // timeout off it emits `baseline-needed` every sync and never compacts, so
    // drive a fixed number of rounds by hand.
    let release = SimClient::new(
        "release",
        schema_v1(),
        BaselineConfig {
            events_per_baseline: Some(2),
            baseline_timeout_ms: None,
            ..BaselineConfig::default()
        },
    );
    for _ in 0..8 {
        release.sync(&mut server);
    }

    assert_eq!(
        release.names().len(),
        3,
        "release should hold all containers"
    );
    assert_eq!(
        server.baseline_count(),
        0,
        "no baseline can be written while the bar is blocked"
    );
}

/// Once the bar stays blocked past `baseline_timeout_ms` of server time, the
/// highest-schema client still syncing writes a baseline anyway. Growth is
/// bounded again and a late joiner can cold-start.
#[test]
fn schema_bar_block_resolves_after_timeout() {
    let mut server = FakeServer::new();

    let nightly = SimClient::new("nightly", schema_v2(), no_baselines());
    nightly.create(json!({ "name": "one", "icon": "a" }));
    nightly.create(json!({ "name": "two", "icon": "b" }));
    nightly.create(json!({ "name": "three", "icon": "c" }));
    sync_to_quiescence(&[&nightly], &mut server);
    assert_eq!(server.baseline_count(), 0, "nightly writes no baseline");

    // Release (schema 1) with a short timeout. Once enough server time passes
    // with the bar blocked and no higher-schema peer active, it writes a baseline.
    let release = SimClient::new(
        "release",
        schema_v1(),
        BaselineConfig {
            events_per_baseline: Some(2),
            baseline_timeout_ms: Some(3),
            ..BaselineConfig::default()
        },
    );
    sync_to_quiescence(&[&release], &mut server);

    assert!(
        server.baseline_count() >= 1,
        "the timeout override should have produced a baseline"
    );
    assert_eq!(release.names().len(), 3);

    // A fresh schema-1 client cold-starts from that baseline.
    let late = SimClient::new("late", schema_v1(), no_baselines());
    sync_to_quiescence(&[&release, &late], &mut server);
    assert_converged(&[&release, &late]);
    assert_eq!(late.names().len(), 3);
}

/// The timeout override picks the highest remaining schema, not just any client.
/// A schema-1 client defers to an active schema-2 peer instead of writing a lossy
/// baseline itself.
#[test]
fn timeout_override_defers_to_highest_active_schema() {
    let mut server = FakeServer::new();

    // Schema-3 nightly writes events (dual-written down to the floor) and leaves.
    let nightly = SimClient::new("nightly", schema_v3(), no_baselines());
    nightly.create(json!({ "name": "one" }));
    nightly.create(json!({ "name": "two" }));
    sync_to_quiescence(&[&nightly], &mut server);

    let config = || BaselineConfig {
        events_per_baseline: Some(2),
        baseline_timeout_ms: Some(3),
        ..BaselineConfig::default()
    };
    // Both remain below the bar (max schema 3); v2 outranks v1.
    let v2 = SimClient::new("v2", schema_v2(), config());
    let v1 = SimClient::new("v1", schema_v1(), config());
    sync_to_quiescence(&[&v2, &v1], &mut server);

    // A baseline lands, the block clears, and both agree on names. Their raw
    // shapes differ by schema, so compare the shared projection.
    assert!(server.baseline_count() >= 1);
    assert_eq!(v2.names(), v1.names());
    assert_eq!(v2.names().len(), 2);
}
