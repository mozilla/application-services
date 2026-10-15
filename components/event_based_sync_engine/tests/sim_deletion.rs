/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Deletion scenarios: a baseline's writer deletes the records it supersedes
//! from the server, clients that missed those records still converge, and a
//! writer that never gets to delete leaves its records to the next writer.
//! `baseline-needed` markers go with the baseline that answers them.

mod support;

use event_based_sync_engine::BaselineConfig;
use serde_json::json;
use support::{
    assert_converged, no_baselines, schema_v1, schema_v2, sync_to_quiescence, FakeServer, Fault,
    SimClient,
};

fn every_three() -> BaselineConfig {
    BaselineConfig {
        events_per_baseline: Some(3),
        baseline_timeout_ms: None,
    }
}

#[test]
fn writer_deletes_superseded_records() {
    let mut server = FakeServer::new();
    let a = SimClient::new("A", schema_v1(), every_three());

    let id = a.create(json!({ "name": "one" }));
    a.create(json!({ "name": "two" }));
    a.modify(&id, "name", json!("uno"));
    sync_to_quiescence(&[&a], &mut server);

    // Only the baseline is left: every event it covers is gone.
    assert!(server.baseline_count() >= 1);
    assert_eq!(server.record_count(), server.baseline_count());

    // A late joiner rebuilds everything from the baseline alone.
    let b = SimClient::new("B", schema_v1(), no_baselines());
    sync_to_quiescence(&[&a, &b], &mut server);
    assert_converged(&[&a, &b]);
    assert_eq!(b.state().len(), 2);
}

#[test]
fn split_baseline_deletes_older_baseline_frames() {
    let mut server = FakeServer::new();
    // A tiny budget frames every baseline, so old frames must be deleted too.
    let a = SimClient::with_record_budget("A", schema_v1(), every_three(), 200);

    for i in 0..6 {
        a.create(json!({ "name": format!("c{i}") }));
    }
    sync_to_quiescence(&[&a], &mut server);
    let first = server.baseline_count();
    assert!(first > 1, "the baseline should be framed, got {first}");

    // A second baseline supersedes the first one's frames.
    for i in 6..12 {
        a.create(json!({ "name": format!("c{i}") }));
    }
    sync_to_quiescence(&[&a], &mut server);
    assert_eq!(server.record_count(), server.baseline_count());

    let b = SimClient::new("B", schema_v1(), no_baselines());
    sync_to_quiescence(&[&a, &b], &mut server);
    assert_converged(&[&a, &b]);
    assert_eq!(b.state().len(), 12);
}

#[test]
fn behind_client_rebuilds_from_baseline_after_events_are_deleted() {
    let mut server = FakeServer::new();
    let a = SimClient::new("A", schema_v1(), every_three());
    let b = SimClient::new("B", schema_v1(), no_baselines());

    let id = a.create(json!({ "name": "one" }));
    sync_to_quiescence(&[&a, &b], &mut server);
    assert_converged(&[&a, &b]);

    // B goes offline. A edits enough to write a baseline and delete the edits.
    a.modify(&id, "name", json!("uno"));
    let other = a.create(json!({ "name": "two" }));
    a.delete(&other);
    a.create(json!({ "name": "three" }));
    sync_to_quiescence(&[&a], &mut server);
    assert_eq!(server.record_count(), server.baseline_count());

    // B never saw those edits; only the baseline can bring it up to date.
    sync_to_quiescence(&[&a, &b], &mut server);
    assert_converged(&[&a, &b]);
    assert_eq!(b.names().get(id.as_str()).map(String::as_str), Some("uno"));
}

#[test]
fn behind_client_keeps_its_unsynced_edits_through_a_rebuild() {
    let mut server = FakeServer::new();
    let a = SimClient::new("A", schema_v1(), every_three());
    let b = SimClient::new("B", schema_v1(), no_baselines());

    let id = a.create(json!({ "name": "one" }));
    sync_to_quiescence(&[&a, &b], &mut server);

    // While offline, B renames the container. A moves on and compacts.
    b.modify(&id, "name", json!("from-b"));
    a.create(json!({ "name": "two" }));
    a.create(json!({ "name": "three" }));
    a.create(json!({ "name": "four" }));
    sync_to_quiescence(&[&a], &mut server);

    // B rebuilds from the baseline but must not lose its queued edit, which
    // uploads after A's and so wins everywhere.
    sync_to_quiescence(&[&b, &a], &mut server);
    assert_converged(&[&a, &b]);
    assert_eq!(
        a.names().get(id.as_str()).map(String::as_str),
        Some("from-b")
    );
    assert_eq!(a.state().len(), 4);
}

#[test]
fn next_writer_deletes_what_a_vanished_writer_left_behind() {
    let mut server = FakeServer::new();
    let a = SimClient::new("A", schema_v1(), every_three());

    a.create(json!({ "name": "one" }));
    a.create(json!({ "name": "two" }));
    a.create(json!({ "name": "three" }));
    // A writes a baseline, then disappears before deleting anything.
    for _ in 0..4 {
        a.sync_with(&mut server, Fault::SkipDeletes);
    }
    assert_eq!(server.baseline_count(), 1);
    assert!(server.record_count() > server.baseline_count());

    // B joins, moves on, and writes the next baseline.
    let b = SimClient::new("B", schema_v1(), every_three());
    sync_to_quiescence(&[&b], &mut server);
    b.create(json!({ "name": "four" }));
    b.create(json!({ "name": "five" }));
    b.create(json!({ "name": "six" }));
    sync_to_quiescence(&[&b], &mut server);

    // B's delete also covers A's leftovers and A's baseline.
    assert_eq!(server.baseline_count(), 1);
    assert_eq!(server.record_count(), 1);

    let c = SimClient::new("C", schema_v1(), no_baselines());
    sync_to_quiescence(&[&b, &c], &mut server);
    assert_converged(&[&b, &c]);
    assert_eq!(c.state().len(), 6);
}

#[test]
fn writer_that_loses_its_upload_confirmation_cleans_up_next_time() {
    let mut server = FakeServer::new();
    let a = SimClient::new("A", schema_v1(), every_three());

    a.create(json!({ "name": "one" }));
    a.create(json!({ "name": "two" }));
    a.create(json!({ "name": "three" }));
    a.sync(&mut server);
    // The baseline commits, but A never learns it did, so it never queues the
    // delete. Next sync A downloads its own baseline as if a peer wrote it.
    a.sync_with(&mut server, Fault::LoseUploadConfirmation);
    assert_eq!(server.baseline_count(), 1);
    sync_to_quiescence(&[&a], &mut server);
    assert!(server.record_count() > server.baseline_count());

    // A's next baseline deletes everything the lost one should have.
    a.create(json!({ "name": "four" }));
    a.create(json!({ "name": "five" }));
    a.create(json!({ "name": "six" }));
    sync_to_quiescence(&[&a], &mut server);
    assert_eq!(server.baseline_count(), 1);
    assert_eq!(server.record_count(), 1);

    let b = SimClient::new("B", schema_v1(), no_baselines());
    sync_to_quiescence(&[&a, &b], &mut server);
    assert_converged(&[&a, &b]);
    assert_eq!(b.state().len(), 6);
}

#[test]
fn baseline_deletes_the_markers_it_answers() {
    let mut server = FakeServer::new();
    // A is the only client meeting the schema bar; it writes only on request.
    let a = SimClient::new("A", schema_v2(), no_baselines());
    let b = SimClient::new("B", schema_v1(), every_three());

    a.create(json!({ "name": "one", "icon": "a" }));
    a.create(json!({ "name": "two", "icon": "b" }));
    a.create(json!({ "name": "three", "icon": "c" }));
    sync_to_quiescence(&[&a], &mut server);

    // B is below the bar, so it asks; A answers with a baseline.
    b.sync(&mut server);
    assert_eq!(server.baseline_count(), 0);
    sync_to_quiescence(&[&a, &b], &mut server);

    // Only the baseline is left: the marker went with the events.
    assert_eq!(server.baseline_count(), 1);
    assert_eq!(server.record_count(), 1);
    assert_eq!(a.names(), b.names());
}
