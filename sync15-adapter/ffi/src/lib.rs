/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

extern crate sync15_adapter as sync;
extern crate libc;
extern crate serde_json;

extern crate failure;

use std::ffi::{CStr, CString};

use std::{ptr, mem};

use sync::{
    Payload,
    Sync15Service,
    Sync15ServiceInit,
    IncomingChangeset,
    OutgoingChangeset,
    ServerTimestamp,
    Store,
    ErrorKind
};

use libc::{c_char, c_double, size_t};

fn c_str_to_string(cs: *const c_char) -> String {
    let c_str = unsafe { CStr::from_ptr(cs) };
    let r_str = c_str.to_str().unwrap_or("");
    r_str.to_string()
}

fn string_to_c_str(s: String) -> *mut c_char {
    CString::new(s).unwrap().into_raw()
}

fn drop_c_str(cs: *mut c_char) {
    if !cs.is_null() {
        unsafe {
            CString::from_raw(cs);
        }
    }
}

fn drop_and_null_c_str(cs: &mut *mut c_char) {
    drop_c_str(mem::replace(cs, ptr::null_mut()));
}

#[repr(C)]
pub struct CleartextBsoC {
    pub server_modified: c_double,
    pub payload_str: *mut c_char,
}

impl Drop for CleartextBsoC {
    fn drop(&mut self) {
        drop_and_null_c_str(&mut self.payload_str);
    }
}

impl CleartextBsoC {
    pub fn new(bso_data: &(Payload, ServerTimestamp)) -> Box<CleartextBsoC> {
        Box::new(CleartextBsoC {
            payload_str: string_to_c_str(bso_data.0.clone().into_json_string()),
            server_modified: bso_data.1.into(),
        })
    }
}

/// Create a new Sync15Service instance.
#[no_mangle]
pub extern "C" fn sync15_service_create(
    key_id: *const c_char,
    access_token: *const c_char,
    sync_key: *const c_char,
    tokenserver_base_url: *const c_char
) -> *mut Sync15Service {
    let params = Sync15ServiceInit {
        key_id: c_str_to_string(key_id),
        access_token: c_str_to_string(access_token),
        sync_key: c_str_to_string(sync_key),
        tokenserver_base_url: c_str_to_string(tokenserver_base_url),
    };
    let mut boxed = match Sync15Service::new(params) {
        Ok(svc) => Box::new(svc),
        Err(e) => {
            eprintln!("Unexpected error initializing Sync15Service: {}", e);
            // TODO: have thoughts about error handling.
            return ptr::null_mut();
        }
    };
    if let Err(e) = boxed.remote_setup() {
        eprintln!("Unexpected error performing remote sync setup: {}", e);
        // TODO: have thoughts about error handling here too.
        return ptr::null_mut();
    }
    Box::into_raw(boxed)
}

/// Free a `Sync15Service` returned by `sync15_service_create`
#[no_mangle]
pub unsafe extern "C" fn sync15_service_destroy(svc: *mut Sync15Service) {
    let _ = Box::from_raw(svc);
}

/// Free an inbound changeset previously returned by `sync15_incoming_changeset_fetch`
#[no_mangle]
pub unsafe extern "C" fn sync15_incoming_changeset_destroy(changeset: *mut IncomingChangeset) {
    let _ = Box::from_raw(changeset);
}

#[no_mangle]
pub unsafe extern "C" fn sync15_outgoing_changeset_destroy(changeset: *mut OutgoingChangeset) {
    let _ = Box::from_raw(changeset);
}

/// Free a record previously returned by `sync15_changeset_get_record_at`.
#[no_mangle]
pub unsafe extern "C" fn sync15_record_destroy(bso: *mut CleartextBsoC) {
    let _ = Box::from_raw(bso);
}

/// Create a new outgoing changeset, which requires that the server have not been
/// modified since it returned the provided `timestamp`.
#[no_mangle]
pub extern "C" fn sync15_outbound_changeset_create(
    collection: *const c_char,
    timestamp: c_double,
) -> *mut OutgoingChangeset {
    assert!(timestamp >= 0.0);
    Box::into_raw(Box::new(
        OutgoingChangeset::new(c_str_to_string(collection),
                               ServerTimestamp(timestamp))))
}

/// Get all the changes for the requested collection that have occurred since last_sync.
/// Important: Caller frees!
#[no_mangle]
pub extern "C" fn sync15_incoming_changeset_fetch(
    svc: *const Sync15Service,
    collection_c: *const c_char,
    last_sync: c_double,
) -> *mut IncomingChangeset {
    let service = unsafe { &*svc };
    let collection = c_str_to_string(collection_c);
    let result = IncomingChangeset::fetch(service, collection, ServerTimestamp(last_sync as f64));
    let fetched = match result {
        Ok(r) => Box::new(r),
        Err(e) => {
            eprintln!("Unexpected error fetching collection: {}", e);
            return ptr::null_mut();
        }
    };
    Box::into_raw(fetched)
}

/// Get the last_sync timestamp for an inbound changeset.
#[no_mangle]
pub extern "C" fn sync15_incoming_changeset_get_timestamp(changeset: *const IncomingChangeset) -> c_double {
    let changeset = unsafe { &*changeset };
    changeset.timestamp.0 as c_double
}

/// Get the number of records from an inbound changeset.
#[no_mangle]
pub extern "C" fn sync15_incoming_changeset_get_len(changeset: *const IncomingChangeset) -> size_t {
    let changeset = unsafe { &*changeset };
    changeset.changes.len()
}

/// Get the requested record from the changeset. `index` should be less than
/// `sync15_changeset_get_record_count`, or NULL will be returned and a
/// message logged to stderr.
///
/// Important: Caller needs to free the returned value using `sync15_record_destroy`
#[no_mangle]
pub extern "C" fn sync15_incoming_changeset_get_at(
    changeset: *const IncomingChangeset,
    index: size_t
) -> *mut CleartextBsoC {
    let changeset = unsafe { &*changeset };
    if index >= changeset.changes.len() {
        eprintln!("sync15_changeset_get_record_at was given an invalid index");
        return ptr::null_mut();
    }
    Box::into_raw(CleartextBsoC::new(&changeset.changes[index]))
}

fn c_str_to_payload(json: *const c_char) -> sync::Result<Payload> {
    let s = unsafe { CStr::from_ptr(json) };
    Ok(Payload::from_json(
        serde_json::from_slice(s.to_bytes())?)?)
}

/// Add a record to an outgoing changeset. Returns false in the case that
/// we were unable to add the record for some reason (typically the json
/// string provided was not well-formed json).
///
/// Note that The `record_json` should only be the record payload, and
/// should not include the BSO envelope.
#[no_mangle]
pub extern "C" fn sync15_outgoing_changeset_add_record(
    changeset: *mut OutgoingChangeset,
    record_json: *const c_char
) -> bool {
    let changeset = unsafe { &mut *changeset };
    assert!(!record_json.is_null());
    let cleartext = match c_str_to_payload(record_json) {
        Ok(ct) => ct,
        Err(e) => {
            eprintln!("Could not add record to changeset: {}", e);
            return false;
        }
    };
    changeset.changes.push(cleartext);
    true
}

/// Add a tombstone to an outgoing changeset. This is equivalent to using
/// `sync15_outgoing_changeset_add_record` with a record that represents a tombstone.
#[no_mangle]
pub extern "C" fn sync15_outgoing_changeset_add_tombstone(
    changeset: *mut OutgoingChangeset,
    record_id: *const c_char
) {
    let changeset = unsafe { &mut *changeset };
    let payload = Payload::new_tombstone(c_str_to_string(record_id).into());
    changeset.changes.push(payload);
}

pub type StoreApplyIncoming = unsafe extern "C" fn(
    self_: *mut libc::c_void,
    incoming: *const IncomingChangeset
) -> *mut OutgoingChangeset;

pub type StoreSyncFinished = unsafe extern "C" fn(
    self_: *mut libc::c_void,
    new_last_sync: c_double,
    synced_ids: *const *const c_char,
    num_synced_ids: size_t
) -> bool;

#[repr(C)]
pub struct FFIStore {
    user_data: *mut libc::c_void,
    apply_incoming_cb: StoreApplyIncoming,
    sync_finished_cb: StoreSyncFinished,
}

struct DropCStrs<'a>(&'a mut [*mut c_char]);
impl<'a> Drop for DropCStrs<'a> {
    fn drop(&mut self) {
        for &c in self.0.iter() {
            drop_c_str(c);
        }
    }
}

impl FFIStore {
    fn call_apply_incoming(
        &self,
        incoming: &IncomingChangeset
    ) -> sync::Result<Box<OutgoingChangeset>> {
        let this = self.user_data;
        let res = unsafe {
            (self.apply_incoming_cb)(this, incoming as *const IncomingChangeset)
        };
        if res.is_null() {
            return Err(ErrorKind::StoreError(
                failure::err_msg("FFI store failed to apply and fetch changes")).into());
        }
        Ok(unsafe { Box::from_raw(res) })
    }

    fn call_sync_finished(&self, ls: ServerTimestamp, ids: &[*mut c_char]) -> sync::Result<()> {
        let this = self.user_data;
        let ok = unsafe {
            let ptr_to_mut: *const *mut c_char = ids.as_ptr();
            let ptr_as_const: *const *const c_char = mem::transmute(ptr_to_mut);
            (self.sync_finished_cb)(this, ls.0 as c_double, ptr_as_const, ids.len() as size_t)
        };
        if !ok {
            return Err(ErrorKind::StoreError(
                failure::err_msg("FFI store failed to note sync finished")).into());
        }
        Ok(())
    }
}

// TODO: better error handling...
impl Store for FFIStore {
    type Error = sync::Error;

    fn apply_incoming(&mut self, incoming: IncomingChangeset) -> sync::Result<OutgoingChangeset> {
        Ok(*self.call_apply_incoming(&incoming)?)
    }

    /// Called when a sync finishes successfully. The store should remove all items in
    /// `synced_ids` from the set of items that need to be synced. and update
    fn sync_finished(&mut self, new_last_sync: ServerTimestamp, synced_ids: &[String]) -> sync::Result<()> {
        let mut buf = Vec::with_capacity(synced_ids.len());
        for id in synced_ids {
            buf.push(CString::new(&id[..]).unwrap().into_raw());
        }
        // More verbose but less error prone than just cleaning up at the end.
        let dropper = DropCStrs(&mut buf);
        self.call_sync_finished(new_last_sync, &dropper.0)?;
        Ok(())
    }
}

#[no_mangle]
pub extern "C" fn sync15_store_create(
    user_data: *mut libc::c_void,
    apply_incoming_cb: StoreApplyIncoming,
    sync_finished_cb: StoreSyncFinished,
) -> *mut FFIStore {
    let store = Box::new(FFIStore {
        user_data,
        apply_incoming_cb,
        sync_finished_cb
    });
    Box::into_raw(store)
}

#[no_mangle]
pub unsafe extern "C" fn sync15_store_destroy(store: *mut FFIStore) {
    assert!(!store.is_null());
    let _ = Box::from_raw(store);
}

#[no_mangle]
pub extern "C" fn sync15_synchronize(
    svc: *const Sync15Service,
    store: *mut FFIStore,
    collection: *const c_char,
    timestamp: c_double,
    fully_atomic: bool
) -> bool {
    assert!(!svc.is_null());
    assert!(!store.is_null());
    let svc = unsafe { &*svc };
    let store = unsafe { &mut *store };
    sync::synchronize(
        svc,
        store,
        c_str_to_string(collection),
        ServerTimestamp(timestamp as f64),
        fully_atomic
    ).is_ok()
}
