/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Maintaining `meta/syncids` when using per-device engine choices.
//!
//! Clients using per-device engine choices never add or remove engines in
//! `meta/global` - adding one would enable it on every client using per-account
//! choices. So the sync IDs for engines missing from `meta/global` live in
//! `meta/syncids` instead. An engine in `meta/global` always uses that entry,
//! which we copy into `meta/syncids` so it survives the engine being removed
//! from `meta/global` by a client using per-account choices.

use super::state::{GlobalState, fetch_sync_ids};
use super::storage_client::SetupStorageClient;
use crate::Guid;
use crate::error::{self, Error, ErrorResponse, info};
use crate::record_types::{MetaGlobalEngine, MetaGlobalRecord, SyncIdsRecord};

/// Works out how `meta/syncids` needs to change so every engine in `engines`
/// has a sync ID, returning the new record if anything changed.
pub(crate) fn plan_sync_ids(
    global: &MetaGlobalRecord,
    current: Option<&SyncIdsRecord>,
    engines: &[(String, usize)],
) -> Option<SyncIdsRecord> {
    // A record for a different meta/global is from a previous generation of
    // the account, so is replaced rather than updated.
    let mut record = match current {
        Some(r) if r.is_usable_for(global) => r.clone(),
        _ => SyncIdsRecord::new_for(global),
    };
    let mut changed = false;
    for (name, version) in engines {
        match global.engines.get(name) {
            Some(mg) => {
                if record.engines.get(name) != Some(mg) {
                    record.engines.insert(name.clone(), mg.clone());
                    changed = true;
                }
            }
            None => {
                if record
                    .engines
                    .get(name)
                    .is_none_or(|e| e.sync_id.is_empty())
                {
                    info!("Minting a new sync ID for the {} engine", name);
                    record.engines.insert(
                        name.clone(),
                        MetaGlobalEngine {
                            version: *version,
                            sync_id: Guid::random(),
                        },
                    );
                    changed = true;
                }
            }
        }
    }
    changed.then_some(record)
}

/// Ensures every engine in `engines` has a sync ID, updating `meta/syncids` if
/// necessary.
pub(crate) fn ensure_sync_ids(
    client: &dyn SetupStorageClient,
    state: &mut GlobalState,
    engines: &[(String, usize)],
) -> error::Result<()> {
    let mut retried = false;
    while let Some(record) = plan_sync_ids(&state.global, state.sync_ids.as_ref(), engines) {
        match client.put_sync_ids(state.sync_ids_timestamp, &record) {
            Ok(timestamp) => {
                state.sync_ids = Some(record);
                state.sync_ids_timestamp = timestamp;
                // We deliberately leave `meta_timestamp` alone - meta/global
                // might have changed since we read it, and this way our next
                // sync will check.
                break;
            }
            Err(Error::StorageHttpError(ErrorResponse::PreconditionFailed { .. })) if !retried => {
                info!("meta/syncids changed while we were updating it, trying again");
                retried = true;
                (state.sync_ids, state.sync_ids_timestamp) = fetch_sync_ids(client)?;
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ServerTimestamp;
    use crate::bso::{IncomingEncryptedBso, OutgoingEncryptedBso};
    use crate::client::request::{InfoCollections, InfoConfiguration};
    use crate::client::storage_client::Sync15ClientResponse;
    use std::cell::RefCell;
    use std::collections::HashMap;

    const GLOBAL_SYNC_ID: &str = "globalAAAAAA";

    fn engine(version: usize, sync_id: &str) -> MetaGlobalEngine {
        MetaGlobalEngine {
            version,
            sync_id: sync_id.into(),
        }
    }

    fn global(engines: &[(&str, usize, &str)]) -> MetaGlobalRecord {
        MetaGlobalRecord {
            sync_id: GLOBAL_SYNC_ID.into(),
            storage_version: 5,
            engines: engines
                .iter()
                .map(|(n, v, id)| (n.to_string(), engine(*v, id)))
                .collect(),
            declined: vec!["bookmarks".into()],
        }
    }

    fn record(engines: &[(&str, usize, &str)]) -> SyncIdsRecord {
        SyncIdsRecord {
            version: 1,
            global_sync_id: Some(GLOBAL_SYNC_ID.into()),
            engines: engines
                .iter()
                .map(|(n, v, id)| (n.to_string(), engine(*v, id)))
                .collect(),
            ..Default::default()
        }
    }

    fn engines(names: &[(&str, usize)]) -> Vec<(String, usize)> {
        names.iter().map(|(n, v)| (n.to_string(), *v)).collect()
    }

    #[test]
    fn test_meta_global_wins_and_is_mirrored() {
        let g = global(&[("history", 1, "historyMGMG")]);
        // Not in the registry yet.
        let updated = plan_sync_ids(&g, None, &engines(&[("history", 1)])).unwrap();
        assert_eq!(updated.engines["history"], engine(1, "historyMGMG"));
        assert!(updated.is_usable_for(&g));
        // In the registry with a different ID: meta/global wins.
        let current = record(&[("history", 1, "historyREGI")]);
        let updated = plan_sync_ids(&g, Some(&current), &engines(&[("history", 1)])).unwrap();
        assert_eq!(updated.engines["history"], engine(1, "historyMGMG"));
        // Already mirrored: nothing to do.
        let current = record(&[("history", 1, "historyMGMG")]);
        assert_eq!(
            plan_sync_ids(&g, Some(&current), &engines(&[("history", 1)])),
            None
        );
    }

    #[test]
    fn test_uses_registry_then_mints() {
        // Note `bookmarks` is declined in meta/global, which we ignore.
        let g = global(&[]);
        let current = record(&[
            ("bookmarks", 2, "bookmarksREG"),
            ("other", 1, "otherREGIDAA"),
        ]);
        assert_eq!(
            plan_sync_ids(&g, Some(&current), &engines(&[("bookmarks", 2)])),
            None
        );

        let updated = plan_sync_ids(
            &g,
            Some(&current),
            &engines(&[("bookmarks", 2), ("tabs", 1)]),
        )
        .unwrap();
        assert_eq!(updated.engines["bookmarks"], engine(2, "bookmarksREG"));
        // Entries for other engines are kept.
        assert_eq!(updated.engines["other"], engine(1, "otherREGIDAA"));
        assert_eq!(updated.engines["tabs"].version, 1);
        assert!(!updated.engines["tabs"].sync_id.is_empty());
    }

    #[test]
    fn test_versions_are_ignored() {
        let g = global(&[("history", 2, "historyMGMG")]);
        let current = record(&[("tabs", 2, "tabsREGIDAAA")]);
        let updated =
            plan_sync_ids(&g, Some(&current), &engines(&[("history", 1), ("tabs", 1)])).unwrap();
        // history is mirrored as-is, and tabs is used as-is.
        assert_eq!(updated.engines["history"], engine(2, "historyMGMG"));
        assert_eq!(updated.engines["tabs"], engine(2, "tabsREGIDAAA"));
    }

    #[test]
    fn test_replaces_unusable_entries_and_records() {
        let g = global(&[]);
        let current = record(&[("tabs", 1, "")]);
        let updated = plan_sync_ids(&g, Some(&current), &engines(&[("tabs", 1)])).unwrap();
        assert!(!updated.engines["tabs"].sync_id.is_empty());

        // A record for a previous generation of the account, or in a format we
        // don't understand, is replaced.
        let mut stale = record(&[("tabs", 1, "tabsREGIDAAA"), ("other", 1, "otherREGIDAA")]);
        stale.global_sync_id = Some("globalOLDOLD".into());
        let mut future = record(&[("tabs", 1, "tabsREGIDAAA"), ("other", 1, "otherREGIDAA")]);
        future.version = 2;
        for current in [stale, future] {
            let updated = plan_sync_ids(&g, Some(&current), &engines(&[("tabs", 1)])).unwrap();
            assert!(updated.is_usable_for(&g));
            assert_ne!(updated.engines["tabs"].sync_id, "tabsREGIDAAA");
            assert!(!updated.engines.contains_key("other"));
        }
    }

    #[test]
    fn test_unknown_fields_are_kept() {
        let g = global(&[]);
        let mut current = record(&[]);
        current
            .extra
            .insert("someFutureField".into(), serde_json::json!(true));
        let updated = plan_sync_ids(&g, Some(&current), &engines(&[("tabs", 1)])).unwrap();
        assert_eq!(updated.extra["someFutureField"], serde_json::json!(true));
    }

    // A client for `ensure_sync_ids`, which only fetches and writes meta/syncids.
    struct SyncIdsClient {
        // The responses to successive PUTs.
        put_results: RefCell<Vec<error::Result<ServerTimestamp>>>,
        // What we PUT, and the X-If-Unmodified-Since we used.
        puts: RefCell<Vec<(ServerTimestamp, SyncIdsRecord)>>,
        // What a refetch returns.
        refetched: Option<SyncIdsRecord>,
    }

    impl SetupStorageClient for SyncIdsClient {
        fn fetch_info_configuration(
            &self,
        ) -> error::Result<Sync15ClientResponse<InfoConfiguration>> {
            unimplemented!()
        }
        fn fetch_info_collections(&self) -> error::Result<Sync15ClientResponse<InfoCollections>> {
            unimplemented!()
        }
        fn fetch_meta_global(&self) -> error::Result<Sync15ClientResponse<MetaGlobalRecord>> {
            unimplemented!()
        }
        fn fetch_crypto_keys(&self) -> error::Result<Sync15ClientResponse<IncomingEncryptedBso>> {
            unimplemented!()
        }
        fn fetch_sync_ids(&self) -> error::Result<Sync15ClientResponse<Option<SyncIdsRecord>>> {
            Ok(Sync15ClientResponse::Success {
                status: 200,
                record: self.refetched.clone(),
                last_modified: ServerTimestamp(2000),
                route: "storage/meta/syncids".into(),
            })
        }
        fn put_meta_global(
            &self,
            _xius: ServerTimestamp,
            _global: &MetaGlobalRecord,
        ) -> error::Result<ServerTimestamp> {
            unimplemented!()
        }
        fn put_sync_ids(
            &self,
            xius: ServerTimestamp,
            sync_ids: &SyncIdsRecord,
        ) -> error::Result<ServerTimestamp> {
            self.puts.borrow_mut().push((xius, sync_ids.clone()));
            self.put_results.borrow_mut().remove(0)
        }
        fn put_crypto_keys(
            &self,
            _xius: ServerTimestamp,
            _keys: &OutgoingEncryptedBso,
        ) -> error::Result<()> {
            unimplemented!()
        }
        fn wipe_all_remote(&self) -> error::Result<()> {
            unimplemented!()
        }
    }

    fn precondition_failed() -> error::Result<ServerTimestamp> {
        Err(Error::StorageHttpError(ErrorResponse::PreconditionFailed {
            route: "storage/meta/syncids".into(),
        }))
    }

    fn state(sync_ids: Option<SyncIdsRecord>) -> GlobalState {
        GlobalState {
            config: InfoConfiguration::default(),
            collections: InfoCollections::new(HashMap::new()),
            global: global(&[]),
            global_timestamp: ServerTimestamp(500),
            meta_timestamp: ServerTimestamp(1000),
            keys: crate::EncryptedPayload {
                iv: String::new(),
                hmac: String::new(),
                ciphertext: String::new(),
            },
            keys_timestamp: ServerTimestamp(500),
            per_device: true,
            sync_ids_timestamp: if sync_ids.is_some() {
                ServerTimestamp(1000)
            } else {
                ServerTimestamp(0)
            },
            sync_ids,
        }
    }

    #[test]
    fn test_ensure_writes_and_updates_state() {
        let client = SyncIdsClient {
            put_results: RefCell::new(vec![Ok(ServerTimestamp(3000))]),
            puts: RefCell::new(vec![]),
            refetched: None,
        };
        let mut s = state(None);
        ensure_sync_ids(&client, &mut s, &engines(&[("tabs", 1)])).unwrap();
        let puts = client.puts.borrow();
        assert_eq!(puts.len(), 1);
        // No record yet, so we insist there still isn't one.
        assert_eq!(puts[0].0, ServerTimestamp(0));
        assert_eq!(s.sync_ids.as_ref(), Some(&puts[0].1));
        assert_eq!(s.sync_ids_timestamp, ServerTimestamp(3000));
        assert_eq!(s.engine_meta("tabs"), Some(&puts[0].1.engines["tabs"]));
        assert_eq!(s.meta_timestamp, ServerTimestamp(1000));

        // And nothing more to do next time.
        ensure_sync_ids(&client, &mut s, &engines(&[("tabs", 1)])).unwrap();
        assert_eq!(client.puts.borrow().len(), 1);
    }

    #[test]
    fn test_ensure_merges_after_conflict() {
        // Someone else added `history` while we were adding `tabs`.
        let client = SyncIdsClient {
            put_results: RefCell::new(vec![precondition_failed(), Ok(ServerTimestamp(3000))]),
            puts: RefCell::new(vec![]),
            refetched: Some(record(&[("history", 1, "historyREGI")])),
        };
        let mut s = state(None);
        ensure_sync_ids(&client, &mut s, &engines(&[("tabs", 1), ("history", 1)])).unwrap();
        let puts = client.puts.borrow();
        assert_eq!(puts.len(), 2);
        assert_eq!(puts[1].0, ServerTimestamp(2000));
        let written = &puts[1].1;
        assert_eq!(written.engines["history"], engine(1, "historyREGI"));
        assert!(written.engines.contains_key("tabs"));
        assert_eq!(s.sync_ids.as_ref(), Some(written));
    }

    #[test]
    fn test_ensure_gives_up_after_second_conflict() {
        let client = SyncIdsClient {
            put_results: RefCell::new(vec![precondition_failed(), precondition_failed()]),
            puts: RefCell::new(vec![]),
            refetched: None,
        };
        let mut s = state(None);
        assert!(ensure_sync_ids(&client, &mut s, &engines(&[("tabs", 1)])).is_err());
        assert_eq!(client.puts.borrow().len(), 2);
        assert!(s.engine_meta("tabs").is_none());
    }
}
