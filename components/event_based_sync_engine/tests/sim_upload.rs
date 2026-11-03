/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Upload failure scenarios: a local edit whose upload fails or is never
//! confirmed still reaches every client, exactly once, on a later sync.

mod support;

use serde_json::json;
use support::{
    assert_converged, no_baselines, schema_v1, sync_to_quiescence, FakeServer, Fault, SimClient,
};

#[test]
fn failed_upload_is_resent_next_sync() {
    let mut server = FakeServer::new();
    let a = SimClient::new("A", schema_v1(), no_baselines());
    let b = SimClient::new("B", schema_v1(), no_baselines());
    let id = a.create(json!({ "name": "work" }));
    sync_to_quiescence(&[&a, &b], &mut server);

    a.modify(&id, "name", json!("renamed"));
    a.sync_with(&mut server, Fault::UploadFails);
    sync_to_quiescence(&[&a, &b], &mut server);

    assert_converged(&[&a, &b]);
    assert_eq!(b.state()[&id]["name"], json!("renamed"));
}

#[test]
fn unconfirmed_upload_that_landed_is_not_resent() {
    let mut server = FakeServer::new();
    let a = SimClient::new("A", schema_v1(), no_baselines());
    let b = SimClient::new("B", schema_v1(), no_baselines());
    let id = a.create(json!({ "name": "work" }));
    sync_to_quiescence(&[&a, &b], &mut server);

    // The server commits the rename, but A never hears back.
    a.modify(&id, "name", json!("renamed"));
    a.sync_with(&mut server, Fault::LoseUploadConfirmation);
    let uploads = server.upload_count();

    // A finds its own record in the next download instead of sending it again.
    sync_to_quiescence(&[&a, &b], &mut server);
    assert_eq!(server.upload_count(), uploads);
    assert_converged(&[&a, &b]);
    assert_eq!(b.state()[&id]["name"], json!("renamed"));
}

#[test]
fn newer_edit_replaces_a_failed_one_to_the_same_field() {
    // Both would upload in one batch and tie on `modified`, leaving the random
    // record id to pick the winner. Repeat to cover both id orderings.
    for _ in 0..20 {
        let mut server = FakeServer::new();
        let a = SimClient::new("A", schema_v1(), no_baselines());
        let b = SimClient::new("B", schema_v1(), no_baselines());
        let id = a.create(json!({ "name": "work" }));
        sync_to_quiescence(&[&a, &b], &mut server);

        a.modify(&id, "name", json!("first"));
        a.sync_with(&mut server, Fault::UploadFails);
        a.modify(&id, "name", json!("second"));
        sync_to_quiescence(&[&a, &b], &mut server);

        assert_converged(&[&a, &b]);
        assert_eq!(b.state()[&id]["name"], json!("second"));
    }
}
