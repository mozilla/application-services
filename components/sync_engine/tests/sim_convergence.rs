/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Core convergence scenarios: independent edits union, concurrent same-field
//! edits resolve to one winner everywhere, and deletes stay sticky.

mod support;

use serde_json::json;
use support::{
    assert_converged, no_baselines, schema_v1, sync_to_quiescence, FakeServer, SimClient,
};

#[test]
fn independent_creates_union() {
    let mut server = FakeServer::new();
    let a = SimClient::new("A", schema_v1(), no_baselines());
    let b = SimClient::new("B", schema_v1(), no_baselines());

    a.create(json!({ "name": "work", "color": "blue" }));
    b.create(json!({ "name": "home", "color": "red" }));

    sync_to_quiescence(&[&a, &b], &mut server);

    assert_converged(&[&a, &b]);
    assert_eq!(a.state().len(), 2);
}

#[test]
fn concurrent_modify_resolves_to_one_winner() {
    let mut server = FakeServer::new();
    let a = SimClient::new("A", schema_v1(), no_baselines());
    let b = SimClient::new("B", schema_v1(), no_baselines());

    let id = a.create(json!({ "name": "orig", "color": "blue" }));
    sync_to_quiescence(&[&a, &b], &mut server);

    // Both rename the same container concurrently.
    a.modify(&id, "name", json!("from-A"));
    b.modify(&id, "name", json!("from-B"));
    sync_to_quiescence(&[&a, &b], &mut server);

    // Both agree on one of the two edits.
    assert_converged(&[&a, &b]);
    assert_eq!(a.state().len(), 1);
    let name = a.state()[&id]["name"].as_str().unwrap().to_string();
    assert!(
        name == "from-A" || name == "from-B",
        "unexpected name {name}"
    );
}

#[test]
fn delete_is_sticky_against_concurrent_modify() {
    let mut server = FakeServer::new();
    let a = SimClient::new("A", schema_v1(), no_baselines());
    let b = SimClient::new("B", schema_v1(), no_baselines());

    let id = a.create(json!({ "name": "doomed" }));
    sync_to_quiescence(&[&a, &b], &mut server);

    // A deletes while B renames the same container.
    a.delete(&id);
    b.modify(&id, "name", json!("too-late"));
    sync_to_quiescence(&[&a, &b], &mut server);

    // The delete wins: a modify can't resurrect a deleted entity.
    assert_converged(&[&a, &b]);
    assert!(a.state().is_empty(), "entity should be gone");
}
