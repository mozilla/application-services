/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::debug;
use crate::{ServerTimestamp, telemetry};
use anyhow::Result;

use crate::Guid;
use crate::bso::{IncomingBso, OutgoingBso};

use super::{CollSyncIds, EngineSyncAssociation, SyncEngine};

/// A BridgedEngine acts as a bridge between application-services, rust
/// implemented sync engines and sync engines as defined by Desktop Firefox.
///
/// [Desktop Firefox has an abstract implementation of a Sync
/// Engine](https://searchfox.org/mozilla-central/source/services/sync/modules/engines.js)
/// with a number of functions each engine is expected to override. Engines
/// implemented in Rust use a different shape (specifically, the
/// [SyncEngine](crate::SyncEngine) trait), so this BridgedEngine trait adapts
/// between the 2.
pub trait BridgedEngine: Send + Sync {
    /// Returns the last sync time, in milliseconds, for this engine's
    /// collection. This is called before each sync, to determine the lower
    /// bound for new records to fetch from the server.
    fn last_sync(&self) -> Result<i64>;

    /// Sets the last sync time, in milliseconds. This is called throughout
    /// the sync, to fast-forward the stored last sync time to match the
    /// timestamp on the uploaded records.
    fn set_last_sync(&self, last_sync_millis: i64) -> Result<()>;

    /// Returns the sync ID for this engine's collection. This is only used in
    /// tests.
    fn sync_id(&self) -> Result<Option<String>>;

    /// Resets the sync ID for this engine's collection, returning the new ID.
    /// As a side effect, implementations should reset all local Sync state,
    /// as in `reset`.
    /// (Note that bridged engines never maintain the "global" guid - that's all managed
    /// by the bridged_engine consumer (ie, desktop). bridged_engines only care about
    /// the per-collection one.)
    fn reset_sync_id(&self) -> Result<String>;

    /// Ensures that the locally stored sync ID for this engine's collection
    /// matches the `new_sync_id` from the server. If the two don't match,
    /// implementations should reset all local Sync state, as in `reset`.
    /// This method returns the assigned sync ID, which can be either the
    /// `new_sync_id`, or a different one if the engine wants to force other
    /// devices to reset their Sync state for this collection the next time they
    /// sync.
    fn ensure_current_sync_id(&self, new_sync_id: &str) -> Result<String>;

    /// Tells the tabs engine about recent FxA devices. A bit of a leaky abstraction as it only
    /// makes sense for tabs.
    /// The arg is a json serialized `ClientData` struct.
    fn prepare_for_sync(&self, _client_data: &str) -> Result<()> {
        Ok(())
    }

    /// Indicates that the engine is about to start syncing. This is called
    /// once per sync, and always before `store_incoming`.
    fn sync_started(&self) -> Result<()>;

    /// Stages a batch of incoming Sync records. This is called multiple
    /// times per sync, once for each batch. Implementations can use the
    /// signal to check if the operation was aborted, and cancel any
    /// pending work.
    fn store_incoming(&self, incoming_records: Vec<IncomingBso>) -> Result<()>;

    /// Applies all staged records, reconciling changes on both sides and
    /// resolving conflicts. Returns a list of records to upload.
    fn apply(&self) -> Result<ApplyResults>;

    /// Indicates that the given record IDs were uploaded successfully to the
    /// server. This is called multiple times per sync, once for each batch
    /// upload.
    fn set_uploaded(&self, server_modified_millis: i64, ids: &[Guid]) -> Result<()>;

    /// Indicates that all records have been uploaded. At this point, any record
    /// IDs marked for upload that haven't been passed to `set_uploaded`, can be
    /// assumed to have failed: for example, because the server rejected a record
    /// with an invalid TTL or sort index.
    fn sync_finished(&self) -> Result<()>;

    /// Resets all local Sync state, including any change flags, mirrors, and
    /// the last sync time, such that the next sync is treated as a first sync
    /// with all new local data. Does not erase any local user data.
    fn reset(&self) -> Result<()>;

    /// Erases all local user data for this collection, and any Sync metadata.
    /// This method is destructive, and unused for most collections.
    fn wipe(&self) -> Result<()>;
}

// This is an adaptor trait - the idea is that engines can implement this
// trait along with SyncEngine and get a BridgedEngine for free. It's temporary
// so we can land this trait without needing to update desktop.
// Longer term, we should remove both this trait and BridgedEngine entirely, sucking up
// the breaking change for desktop. The main blocker to this is moving desktop away
// from the explicit timestamp handling and moving closer to the `get_collection_request`
// model.
pub trait BridgedEngineAdaptor: Send + Sync {
    // These are the main mismatches between the 2 engines
    fn last_sync(&self) -> Result<i64>;
    fn set_last_sync(&self, last_sync_millis: i64) -> Result<()>;
    fn sync_started(&self) -> Result<()> {
        Ok(())
    }

    fn engine(&self) -> &dyn SyncEngine;
}

impl<A: BridgedEngineAdaptor> BridgedEngine for A {
    fn last_sync(&self) -> Result<i64> {
        self.last_sync()
    }

    fn set_last_sync(&self, last_sync_millis: i64) -> Result<()> {
        self.set_last_sync(last_sync_millis)
    }

    fn sync_id(&self) -> Result<Option<String>> {
        Ok(match self.engine().get_sync_assoc()? {
            EngineSyncAssociation::Disconnected => None,
            EngineSyncAssociation::Connected(c) => Some(c.coll.into()),
        })
    }

    fn reset_sync_id(&self) -> Result<String> {
        // Note that bridged engines never maintain the "global" guid - that's all managed
        // by desktop. bridged_engines only care about the per-collection one.
        let global = Guid::empty();
        let coll = Guid::random();
        self.engine()
            .reset(&EngineSyncAssociation::Connected(CollSyncIds {
                global,
                coll: coll.clone(),
            }))?;
        Ok(coll.to_string())
    }

    fn ensure_current_sync_id(&self, sync_id: &str) -> Result<String> {
        let engine = self.engine();
        let assoc = engine.get_sync_assoc()?;
        if matches!(assoc, EngineSyncAssociation::Connected(c) if c.coll == sync_id) {
            debug!("ensure_current_sync_id is current");
        } else {
            let new_coll_ids = CollSyncIds {
                global: Guid::empty(),
                coll: sync_id.into(),
            };
            engine.reset(&EngineSyncAssociation::Connected(new_coll_ids))?;
        }
        Ok(sync_id.to_string())
    }

    fn prepare_for_sync(&self, client_data: &str) -> Result<()> {
        // unwrap here is unfortunate, but can hopefully go away if we can
        // start using the ClientData type instead of the string.
        self.engine()
            .prepare_for_sync(&|| serde_json::from_str::<crate::ClientData>(client_data).unwrap())
    }

    fn sync_started(&self) -> Result<()> {
        A::sync_started(self)
    }

    fn store_incoming(&self, incoming_records: Vec<IncomingBso>) -> Result<()> {
        let engine = self.engine();
        let mut telem = telemetry::Engine::new(engine.collection_name());
        engine.stage_incoming(incoming_records, &mut telem)
    }

    fn apply(&self) -> Result<ApplyResults> {
        let engine = self.engine();
        let mut telem = telemetry::Engine::new(engine.collection_name());
        // Desktop tells a bridged engine to apply the records without telling it
        // the server timestamp, and once applied, explicitly calls `set_last_sync()`
        // with that timestamp. So this adaptor needs to call apply with an invalid
        // timestamp, and hope that later call with the correct timestamp does come.
        // This isn't ideal as it means the timestamp is updated in a different transaction,
        // but nothing too bad should happen if it doesn't - we'll just end up applying
        // the same records again next sync.
        let records = engine.apply(ServerTimestamp::from_millis(0), &mut telem)?;
        Ok(ApplyResults {
            records,
            num_reconciled: telem
                .get_incoming()
                .as_ref()
                .map(|i| i.get_reconciled() as usize),
        })
    }

    fn set_uploaded(&self, millis: i64, ids: &[Guid]) -> Result<()> {
        self.engine()
            .set_uploaded(ServerTimestamp::from_millis(millis), ids.to_vec())
    }

    fn sync_finished(&self) -> Result<()> {
        self.engine().sync_finished()
    }

    fn reset(&self) -> Result<()> {
        self.engine().reset(&EngineSyncAssociation::Disconnected)
    }

    fn wipe(&self) -> Result<()> {
        self.engine().wipe()
    }
}

// TODO: We should see if we can remove this to reduce the number of types engines need to deal
// with. num_reconciled is only used for telemetry on desktop.
#[derive(Debug, Default)]
pub struct ApplyResults {
    /// List of records
    pub records: Vec<OutgoingBso>,
    /// The number of incoming records whose contents were merged because they
    /// changed on both sides. None indicates we aren't reporting this
    /// information.
    pub num_reconciled: Option<usize>,
}

impl ApplyResults {
    pub fn new(records: Vec<OutgoingBso>, num_reconciled: impl Into<Option<usize>>) -> Self {
        Self {
            records,
            num_reconciled: num_reconciled.into(),
        }
    }
}

// Shorthand for engines that don't care.
impl From<Vec<OutgoingBso>> for ApplyResults {
    fn from(records: Vec<OutgoingBso>) -> Self {
        Self {
            records,
            num_reconciled: None,
        }
    }
}

/// Wraps a `Box<dyn BridgedEngine>` and centralizes the work every consuming
/// crate's UniFFI-facing bridged engine needs to do: the JSON `String` <-> BSO
/// marshalling that crosses the FFI boundary, and 1:1 delegation to the wrapped
/// engine. Rather than each crate hand-writing this (it was ~100 identical lines
/// per crate), they expose a thin newtype around this via the
/// [`uniffi_bridged_engine!`] macro.
///
/// All methods return [`anyhow::Result`], which each crate maps onto its own
/// UniFFI error type via an `impl From<anyhow::Error>`.
///
/// Note on the longer-term direction: this type, along with [`BridgedEngine`],
/// [`BridgedEngineAdaptor`] and [`ApplyResults`], only exists because we still
/// have two sync-engine traits. Once Desktop moves off explicit timestamp
/// handling to the `get_collection_request` model (see #2841) we can remove
/// `BridgedEngine` entirely, have Desktop consume [`SyncEngine`] directly, and
/// this wrapper collapses into a thin `SyncEngine` -> FFI shim (or goes away).
/// See the note in `engine/mod.rs` for the migration sequencing.
pub struct BridgedEngineWrapper {
    inner: Box<dyn BridgedEngine>,
}

impl BridgedEngineWrapper {
    pub fn new(inner: Box<dyn BridgedEngine>) -> Self {
        Self { inner }
    }

    pub fn last_sync(&self) -> Result<i64> {
        self.inner.last_sync()
    }

    pub fn set_last_sync(&self, last_sync: i64) -> Result<()> {
        self.inner.set_last_sync(last_sync)
    }

    pub fn sync_id(&self) -> Result<Option<String>> {
        self.inner.sync_id()
    }

    pub fn reset_sync_id(&self) -> Result<String> {
        self.inner.reset_sync_id()
    }

    pub fn ensure_current_sync_id(&self, sync_id: &str) -> Result<String> {
        self.inner.ensure_current_sync_id(sync_id)
    }

    pub fn prepare_for_sync(&self, client_data: &str) -> Result<()> {
        self.inner.prepare_for_sync(client_data)
    }

    pub fn sync_started(&self) -> Result<()> {
        self.inner.sync_started()
    }

    /// Decode the JSON-encoded `IncomingBso`s that UniFFI passes to us, then
    /// hand them to the wrapped engine.
    pub fn store_incoming(&self, incoming: Vec<String>) -> Result<()> {
        let mut bsos = Vec::with_capacity(incoming.len());
        for inc in incoming {
            bsos.push(serde_json::from_str::<IncomingBso>(&inc)?);
        }
        self.inner.store_incoming(bsos)
    }

    /// Apply staged records and encode the outgoing `OutgoingBso`s back into
    /// JSON for UniFFI.
    pub fn apply(&self) -> Result<Vec<String>> {
        let apply_results = self.inner.apply()?;
        let mut outgoing = Vec::with_capacity(apply_results.records.len());
        for e in apply_results.records {
            outgoing.push(serde_json::to_string(&e)?);
        }
        Ok(outgoing)
    }

    /// Accepts anything that turns into a [`Guid`], which reconciles the
    /// per-crate id representation: logins hands us `Vec<String>`, while
    /// tabs and webext-storage hand us `Vec<sync_guid::Guid>`. Both `String`
    /// and `Guid` implement `Into<Guid>`.
    pub fn set_uploaded<G: Into<Guid>>(
        &self,
        server_modified_millis: i64,
        ids: Vec<G>,
    ) -> Result<()> {
        let guids: Vec<Guid> = ids.into_iter().map(Into::into).collect();
        self.inner.set_uploaded(server_modified_millis, &guids)
    }

    pub fn sync_finished(&self) -> Result<()> {
        self.inner.sync_finished()
    }

    pub fn reset(&self) -> Result<()> {
        self.inner.reset()
    }

    pub fn wipe(&self) -> Result<()> {
        self.inner.wipe()
    }
}

/// Generates a UniFFI-exposable bridged engine newtype around
/// [`BridgedEngineWrapper`], removing the ~100 lines of identical facade
/// boilerplate each consuming crate used to hand-write.
///
/// Usage (invoke in the module the crate's UDL `interface` resolves against):
/// ```ignore
/// sync15::uniffi_bridged_engine!(LoginsBridgedEngine, String);
/// sync15::uniffi_bridged_engine!(TabsBridgedEngine, sync_guid::Guid);
/// ```
///
/// `$guid` is the element type the crate's UDL lowers `set_uploaded`'s ids to
/// (`String` for logins' `sequence<string>`, `sync_guid::Guid` for the tabs and
/// webext-storage custom-type sequences). The generated methods return
/// `anyhow::Result`, which the crate's UDL `[Throws=...]` maps to its error type
/// via the existing `impl From<anyhow::Error>`.
///
/// The macro always emits `prepare_for_sync`; a crate whose UDL doesn't declare
/// it (logins) simply leaves that inherent method unbound, which is harmless.
#[macro_export]
macro_rules! uniffi_bridged_engine {
    ($name:ident, $guid:ty) => {
        // This is what UniFFI exposes; it does nothing other than delegate to
        // the shared `BridgedEngineWrapper`. See
        // services/interfaces/mozIBridgedSyncEngine.idl for the Desktop contract.
        pub struct $name($crate::engine::BridgedEngineWrapper);

        impl $name {
            pub fn new(inner: ::std::boxed::Box<dyn $crate::engine::BridgedEngine>) -> Self {
                Self($crate::engine::BridgedEngineWrapper::new(inner))
            }

            pub fn last_sync(&self) -> ::anyhow::Result<i64> {
                self.0.last_sync()
            }

            pub fn set_last_sync(&self, last_sync: i64) -> ::anyhow::Result<()> {
                self.0.set_last_sync(last_sync)
            }

            pub fn sync_id(&self) -> ::anyhow::Result<Option<String>> {
                self.0.sync_id()
            }

            pub fn reset_sync_id(&self) -> ::anyhow::Result<String> {
                self.0.reset_sync_id()
            }

            pub fn ensure_current_sync_id(&self, sync_id: &str) -> ::anyhow::Result<String> {
                self.0.ensure_current_sync_id(sync_id)
            }

            pub fn prepare_for_sync(&self, client_data: &str) -> ::anyhow::Result<()> {
                self.0.prepare_for_sync(client_data)
            }

            pub fn sync_started(&self) -> ::anyhow::Result<()> {
                self.0.sync_started()
            }

            pub fn store_incoming(&self, incoming: Vec<String>) -> ::anyhow::Result<()> {
                self.0.store_incoming(incoming)
            }

            pub fn apply(&self) -> ::anyhow::Result<Vec<String>> {
                self.0.apply()
            }

            pub fn set_uploaded(
                &self,
                server_modified_millis: i64,
                ids: Vec<$guid>,
            ) -> ::anyhow::Result<()> {
                self.0.set_uploaded(server_modified_millis, ids)
            }

            pub fn sync_finished(&self) -> ::anyhow::Result<()> {
                self.0.sync_finished()
            }

            pub fn reset(&self) -> ::anyhow::Result<()> {
                self.0.reset()
            }

            pub fn wipe(&self) -> ::anyhow::Result<()> {
                self.0.wipe()
            }
        }
    };
}

#[cfg(test)]
mod wrapper_tests {
    use super::*;
    use crate::bso::OutgoingBso;
    use std::sync::Mutex;

    // A minimal BridgedEngine that records the guids passed to `set_uploaded`,
    // so we can lock in the `Into<Guid>` reconciliation for both `String` and
    // `Guid` element types.
    #[derive(Default)]
    struct RecordingEngine {
        uploaded: Mutex<Vec<Guid>>,
    }

    impl BridgedEngine for RecordingEngine {
        fn last_sync(&self) -> Result<i64> {
            Ok(0)
        }
        fn set_last_sync(&self, _: i64) -> Result<()> {
            Ok(())
        }
        fn sync_id(&self) -> Result<Option<String>> {
            Ok(None)
        }
        fn reset_sync_id(&self) -> Result<String> {
            Ok(String::new())
        }
        fn ensure_current_sync_id(&self, id: &str) -> Result<String> {
            Ok(id.to_string())
        }
        fn sync_started(&self) -> Result<()> {
            Ok(())
        }
        fn store_incoming(&self, _: Vec<IncomingBso>) -> Result<()> {
            Ok(())
        }
        fn apply(&self) -> Result<ApplyResults> {
            Ok(Vec::<OutgoingBso>::new().into())
        }
        fn set_uploaded(&self, _millis: i64, ids: &[Guid]) -> Result<()> {
            self.uploaded.lock().unwrap().extend_from_slice(ids);
            Ok(())
        }
        fn sync_finished(&self) -> Result<()> {
            Ok(())
        }
        fn reset(&self) -> Result<()> {
            Ok(())
        }
        fn wipe(&self) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn set_uploaded_accepts_strings_and_guids() {
        let wrapper = BridgedEngineWrapper::new(Box::new(RecordingEngine::default()));
        // logins-style: Vec<String>
        wrapper.set_uploaded(1, vec!["aaaa".to_string()]).unwrap();
        // tabs/webext-style: Vec<Guid>
        wrapper.set_uploaded(2, vec![Guid::new("bbbb")]).unwrap();
    }
}
