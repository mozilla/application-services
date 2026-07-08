/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This module is used by crates which need to implement a "sync engine".
//! At a high-level, a "sync engine" is code which knows how to take records
//! from a sync server, apply and reconcile them with the local data, then
//! provide records which should be uploaded to the server.
//!
//! Note that the "sync engine" does not itself talk to the server, nor does
//! it manage the state of the remote server, nor does it do any of the
//! encryption/decryption - that is the responsbility of the "sync client", as
//! implemented in the [client] module (or in some cases, implemented externally)
//!
//! There are currently 2 types of engine:
//! * Code which implements the [crate::engine::sync_engine::SyncEngine]
//!   trait. These are the "original" Rust engines, designed to be used with
//!   the [crate::client](sync client)
//! * Code which implements the [crate::engine::bridged_engine::BridgedEngine]
//!   trait. These engines are a "bridge" between the Desktop JS Sync world and
//!   this rust code.
//!
//! While these engines end up doing the same thing, the difference is due to
//! implementation differences between the Desktop Sync client and the Rust
//! client.
//!
//! We intend merging these engines - the first step will be to merge the
//! types and payload management used by these traits, then to combine the
//! requirements into a single trait that captures both use-cases.
//!
//! Steps so far, and what's left:
//! * [bridged_engine::BridgedEngineAdaptor] lets a crate implement only
//!   [SyncEngine] (plus a tiny adaptor) and get a [bridged_engine::BridgedEngine]
//!   for free.
//! * [bridged_engine::BridgedEngineWrapper] + the `uniffi_bridged_engine!` macro
//!   remove the per-crate UniFFI facade boilerplate (the JSON<->BSO marshalling
//!   and method delegation).
//! * Still to do (#2841): remove `BridgedEngine`/`BridgedEngineAdaptor`/`ApplyResults`
//!   entirely and have Desktop consume [SyncEngine] directly. This is blocked on a
//!   coordinated mozilla-central change: Desktop must move off explicit timestamp
//!   handling (`last_sync`/`set_last_sync`) to the `get_collection_request` model,
//!   and the per-crate UDL `interface *BridgedEngine` blocks (the Desktop-visible
//!   contract consumed via mozIBridgedSyncEngine) must be updated in lockstep.
mod bridged_engine;
mod request;
mod sync_engine;

pub use bridged_engine::{ApplyResults, BridgedEngine, BridgedEngineAdaptor, BridgedEngineWrapper};
#[cfg(feature = "sync-client")]
pub(crate) use request::CollectionPost;

pub use request::{CollectionRequest, RequestOrder};
pub use sync_engine::{CollSyncIds, EngineSyncAssociation, SyncEngine, SyncEngineId};
