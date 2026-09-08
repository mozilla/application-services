/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use serde_json::json;

use containers::{
    max_user_context_id, ContainerLabel, ContainersCallback, ContainersStore, InitError,
    StoreError, UserIdentitySpec,
};

#[derive(Default)]
struct Recorded {
    persists: Mutex<usize>,
}

#[derive(Clone, Default)]
struct Recorder {
    inner: Arc<Recorded>,
}

impl Recorder {
    fn persist_count(&self) -> usize {
        *self.inner.persists.lock().unwrap()
    }
}

impl ContainersCallback for Recorder {
    fn persist(&self) {
        *self.inner.persists.lock().unwrap() += 1;
    }
}

fn seeded() -> (ContainersStore, Recorder) {
    let recorder = Recorder::default();
    let store = ContainersStore::new(None, None, Box::new(recorder.clone())).unwrap();
    (store, recorder)
}

fn current_document() -> Vec<u8> {
    serde_json::to_vec(&json!({
        "version": 6,
        "lastUserContextId": 5,
        "identities": [
            { "userContextId": 1, "public": true, "icon": "fingerprint", "color": "blue", "l10nId": "user-context-personal" },
            { "userContextId": 2, "public": true, "icon": "briefcase", "color": "orange", "l10nId": "user-context-work" },
            { "userContextId": 5, "public": false, "icon": "", "color": "", "name": "userContextIdInternal.thumbnail" },
        ],
        "siteAssociations": {},
    }))
    .unwrap()
}

#[test]
fn seeding_from_defaults_persists_once() {
    let (store, recorder) = seeded();

    assert_eq!(store.public_identities().len(), 4);
    assert_eq!(recorder.persist_count(), 1);
}

#[test]
fn loading_a_current_document_does_not_persist() {
    let recorder = Recorder::default();
    let store =
        ContainersStore::new(Some(current_document()), None, Box::new(recorder.clone())).unwrap();

    assert_eq!(store.public_identities().len(), 2);
    assert_eq!(recorder.persist_count(), 0);
}

#[test]
fn loading_a_migrated_document_persists() {
    let recorder = Recorder::default();
    let document = serde_json::to_vec(&json!({
        "version": 5,
        "lastUserContextId": 1,
        "identities": [
            { "userContextId": 1, "public": true, "icon": "gift", "color": "turquoise", "name": "Aliased" },
        ],
    }))
    .unwrap();

    let store = ContainersStore::new(Some(document), None, Box::new(recorder.clone())).unwrap();

    assert_eq!(store.public_identity_from_id(1).unwrap().color, "cyan");
    assert_eq!(recorder.persist_count(), 1);
}

#[test]
fn create_assigns_the_next_id_and_persists() {
    let (store, recorder) = seeded();

    let container = store.create("Reading", "tree", "purple").unwrap();

    assert_eq!(container.user_context_id, 6);
    assert_eq!(
        container.label,
        ContainerLabel::Name {
            name: "Reading".into()
        }
    );
    assert!(container.is_public);
    assert_eq!(store.public_identities().len(), 5);
    assert_eq!(recorder.persist_count(), 2);
}

#[test]
fn create_rejects_a_blank_name() {
    let (store, recorder) = seeded();
    let before = recorder.persist_count();

    assert_eq!(
        store.create("   ", "tree", "purple"),
        Err(StoreError::EmptyName)
    );
    assert_eq!(store.public_identities().len(), 4);
    assert_eq!(recorder.persist_count(), before);
}

#[test]
fn update_replaces_the_label_and_drops_the_fluent_id() {
    let (store, _) = seeded();

    let updated = store.update(1, "Mine", "fence", "red").unwrap().unwrap();

    assert_eq!(
        updated.label,
        ContainerLabel::Name {
            name: "Mine".into()
        }
    );
    assert_eq!(updated.icon, "fence");
    assert_eq!(updated.color, "red");

    // A non-empty name wins whether or not the Fluent id is still there, so
    // only the stored document can show that it is gone.
    let persisted: serde_json::Value = serde_json::from_slice(&store.serialize()).unwrap();
    let identity = &persisted["identities"][0];
    assert_eq!(identity["name"], json!("Mine"));
    assert!(identity.get("l10nId").is_none());
}

#[test]
fn update_ignores_unknown_and_private_containers() {
    let (store, _) = seeded();

    assert!(store.update(999, "Mine", "fence", "red").unwrap().is_none());
    // The thumbnail identity is private.
    assert!(store.update(5, "Mine", "fence", "red").unwrap().is_none());
}

#[test]
fn remove_drops_the_container_and_its_associations() {
    let (store, _) = seeded();
    store.set_site_association("example.org", 1).unwrap();
    store.set_site_association("example.com", 2).unwrap();

    let removed = store.remove(1).expect("container 1 should be removed");

    assert_eq!(removed.user_context_id, 1);
    assert_eq!(store.public_identities().len(), 3);
    assert_eq!(store.get_site_association("example.org"), 0);
    assert_eq!(store.get_site_association("example.com"), 2);
}

#[test]
fn remove_ignores_unknown_and_private_containers() {
    let (store, _) = seeded();

    assert!(store.remove(999).is_none());
    assert!(store.remove(5).is_none());
    assert!(store
        .private_identity("userContextIdInternal.webextStorageLocal")
        .is_some());
}

#[test]
fn move_reorders_public_containers() {
    let (store, _) = seeded();

    assert!(store.move_containers(vec![4], 0));

    assert_eq!(
        store.public_user_context_ids(),
        vec![4, 1, 2, 3],
        "the moved container lands at the front"
    );
}

#[test]
fn move_to_minus_one_appends() {
    let (store, _) = seeded();

    assert!(store.move_containers(vec![1], -1));

    assert_eq!(store.public_user_context_ids(), vec![2, 3, 4, 1]);
}

#[test]
fn move_rejects_positions_below_minus_one() {
    let (store, recorder) = seeded();
    let before = recorder.persist_count();

    assert!(!store.move_containers(vec![1], -2));
    assert_eq!(store.public_user_context_ids(), vec![1, 2, 3, 4]);
    assert_eq!(recorder.persist_count(), before);
}

#[test]
fn move_is_a_no_op_without_matching_containers() {
    let (store, recorder) = seeded();
    let before = recorder.persist_count();

    assert!(!store.move_containers(vec![999], 0));
    assert_eq!(recorder.persist_count(), before);
}

#[test]
fn site_associations_round_trip() {
    let (store, _) = seeded();

    store.set_site_association("Example.ORG", 2).unwrap();

    assert_eq!(store.get_site_association("example.org"), 2);
    assert_eq!(store.get_site_association("other.example"), 0);

    store.remove_site_association("example.org");
    assert_eq!(store.get_site_association("example.org"), 0);
}

#[test]
fn setting_the_same_association_twice_does_not_persist() {
    let (store, recorder) = seeded();
    store.set_site_association("example.org", 2).unwrap();
    let after_first = recorder.persist_count();

    store.set_site_association("example.org", 2).unwrap();

    assert_eq!(recorder.persist_count(), after_first);
}

#[test]
fn associations_require_a_known_public_container() {
    let (store, _) = seeded();

    assert_eq!(
        store.set_site_association("example.org", 999),
        Err(StoreError::NoSuchContainer {
            user_context_id: 999
        })
    );
    // The thumbnail identity is private.
    assert_eq!(
        store.set_site_association("example.org", 5),
        Err(StoreError::NoSuchContainer { user_context_id: 5 })
    );
}

#[test]
fn wildcards_are_not_valid_sites() {
    let (store, _) = seeded();

    assert_eq!(
        store.set_site_association("*.example.org", 1),
        Err(StoreError::InvalidSite)
    );
}

#[test]
fn get_site_associations_filters_by_container() {
    let (store, _) = seeded();
    store.set_site_association("one.example", 1).unwrap();
    store.set_site_association("two.example", 2).unwrap();

    assert_eq!(store.get_site_associations(None).len(), 2);
    assert_eq!(store.get_site_associations(Some(1)).len(), 1);
    assert_eq!(store.get_site_associations(Some(1))[0].site, "one.example");
}

#[test]
fn container_for_navigation_falls_back_to_the_baseline() {
    let (store, _) = seeded();
    store.set_site_association("example.org", 3).unwrap();

    assert_eq!(store.container_for_navigation("example.org", 0), 3);
    assert_eq!(store.container_for_navigation("example.org", 7), 3);
    assert_eq!(store.container_for_navigation("other.example", 7), 7);
    assert_eq!(store.container_for_navigation("", 7), 7);
}

#[test]
fn mutations_are_visible_in_the_serialized_document() {
    let (store, _) = seeded();

    store.create("Reading", "tree", "purple").unwrap();

    let persisted: serde_json::Value = serde_json::from_slice(&store.serialize()).unwrap();
    assert_eq!(persisted["lastUserContextId"], json!(6));

    // create() appends, so the new container sits after the system identities.
    let identities = persisted["identities"].as_array().unwrap();
    assert_eq!(identities.last().unwrap()["name"], json!("Reading"));
}

#[test]
fn unset_callback_stops_delivery() {
    let (store, recorder) = seeded();
    let before = recorder.persist_count();

    store.unset_callback();
    store.create("Reading", "tree", "purple").unwrap();

    assert_eq!(recorder.persist_count(), before);
    assert_eq!(store.public_identities().len(), 5);
}

fn store_with_last_id(last_user_context_id: u32) -> ContainersStore {
    let document = serde_json::to_vec(&json!({
        "version": 6,
        "lastUserContextId": last_user_context_id,
        "identities": [
            { "userContextId": max_user_context_id(), "public": false, "icon": "", "color": "",
              "name": "userContextIdInternal.webextStorageLocal" },
        ],
    }))
    .unwrap();

    ContainersStore::new(Some(document), None, Box::new(Recorder::default())).unwrap()
}

#[test]
fn the_last_assignable_id_is_the_one_below_the_reserved_one() {
    let store = store_with_last_id(max_user_context_id() - 2);

    let identity = store.create("Last one", "circle", "gray").unwrap();

    assert_eq!(identity.user_context_id, max_user_context_id() - 1);
}

#[test]
fn create_fails_once_the_id_space_is_exhausted() {
    let store = store_with_last_id(max_user_context_id() - 1);

    assert_eq!(
        store.create("One too many", "circle", "gray"),
        Err(StoreError::IdSpaceExhausted)
    );
    assert!(store.public_identities().is_empty());
}

/// Gecko increments before validating, so there a rejected name leaves a hole
/// in the sequence. This pins the deliberate difference.
#[test]
fn a_rejected_name_does_not_consume_an_id() {
    let (store, _) = seeded();

    assert!(store.create("   ", "tree", "purple").is_err());

    assert_eq!(
        store
            .create("Reading", "tree", "purple")
            .unwrap()
            .user_context_id,
        6
    );
}

#[test]
fn an_unknown_container_is_reported_before_an_unusable_site() {
    let (store, _) = seeded();

    assert_eq!(
        store.set_site_association("*.example.org", 999),
        Err(StoreError::NoSuchContainer {
            user_context_id: 999
        })
    );
}

fn reloaded(store: &ContainersStore) -> ContainersStore {
    ContainersStore::new(Some(store.serialize()), None, Box::new(Recorder::default())).unwrap()
}

#[test]
fn every_mutation_leaves_a_document_that_reloads_identically() {
    let (store, _) = seeded();

    store.create("Reading", "tree", "purple").unwrap();
    assert_eq!(
        reloaded(&store).public_identities(),
        store.public_identities()
    );

    store.update(1, "Mine", "fence", "red").unwrap();
    assert_eq!(
        reloaded(&store).public_identities(),
        store.public_identities()
    );

    store.set_site_association("example.org", 2).unwrap();
    assert_eq!(
        reloaded(&store).get_site_associations(None),
        store.get_site_associations(None)
    );

    assert!(store.move_containers(vec![1], -1));
    assert_eq!(
        reloaded(&store).public_user_context_ids(),
        store.public_user_context_ids()
    );

    // Removing container 2 also drops the association bound to it.
    store.remove(2).unwrap();
    let reloaded = reloaded(&store);
    assert_eq!(reloaded.public_identities(), store.public_identities());
    assert_eq!(
        reloaded.get_site_associations(None),
        store.get_site_associations(None)
    );
}

#[test]
fn concurrent_creates_get_distinct_ids() {
    let store = Arc::new(ContainersStore::new(None, None, Box::new(Recorder::default())).unwrap());

    let handles: Vec<_> = (0..8)
        .map(|i| {
            let store = Arc::clone(&store);
            std::thread::spawn(move || {
                store
                    .create(&format!("Container {i}"), "circle", "gray")
                    .unwrap()
                    .user_context_id
            })
        })
        .collect();

    let ids: HashSet<u32> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();

    assert_eq!(ids.len(), 8, "every concurrent create gets its own id");
    assert_eq!(store.public_identities().len(), 12);
}

fn seed(name: &str, icon: &str, color: &str) -> UserIdentitySpec {
    UserIdentitySpec {
        icon: icon.to_string(),
        color: color.to_string(),
        label: ContainerLabel::Name {
            name: name.to_string(),
        },
    }
}

fn seeded_with(spec: UserIdentitySpec) -> Result<ContainersStore, InitError> {
    ContainersStore::new(None, Some(vec![spec]), Box::new(Recorder::default()))
}

#[test]
fn a_seed_identity_rejects_an_unknown_icon_or_color() {
    assert_eq!(
        seeded_with(seed("Work", "spaceship", "blue"))
            .err()
            .unwrap(),
        InitError::InvalidSeedIcon {
            icon: "spaceship".into()
        }
    );
    assert_eq!(
        seeded_with(seed("Work", "briefcase", "chartreuse"))
            .err()
            .unwrap(),
        InitError::InvalidSeedColor {
            color: "chartreuse".into()
        }
    );
}

#[test]
fn a_seed_identity_resolves_legacy_colors() {
    let store =
        seeded_with(seed("Work", "briefcase", "turquoise")).expect("a legacy color is accepted");

    let containers = store.public_identities();
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].color, "cyan");
    assert_eq!(
        containers[0].label,
        ContainerLabel::Name {
            name: "Work".into()
        }
    );
}

/// Mirrors getUserContextLabel: a non-empty name wins, an empty one falls
/// through to the localized id, and a container with neither is not left
/// without a label.
#[test]
fn the_label_follows_geckos_precedence() {
    let document = serde_json::to_vec(&json!({
        "version": 6,
        "lastUserContextId": 3,
        "identities": [
            { "userContextId": 1, "public": true, "icon": "cart", "color": "blue",
              "name": "Renamed", "l10nId": "user-context-personal" },
            { "userContextId": 2, "public": true, "icon": "cart", "color": "blue",
              "name": "", "l10nId": "user-context-work" },
            { "userContextId": 3, "public": true, "icon": "cart", "color": "blue" },
        ],
    }))
    .unwrap();

    let store = ContainersStore::new(Some(document), None, Box::new(Recorder::default())).unwrap();
    let label = |id| store.public_identity_from_id(id).unwrap().label;

    assert_eq!(
        label(1),
        ContainerLabel::Name {
            name: "Renamed".into()
        }
    );
    assert_eq!(
        label(2),
        ContainerLabel::L10nId {
            l10n_id: "user-context-work".into()
        }
    );
    assert_eq!(
        label(3),
        ContainerLabel::Name {
            name: String::new()
        }
    );
}
