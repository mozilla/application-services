/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

extern crate sync15_adapter as sync;
extern crate libc;
extern crate serde_json;

use std::ffi::{CStr, CString};

use std::{ptr, mem};
use sync::{RecordChangeset, CleartextBso, Payload, Sync15Service, Sync15ServiceInit};
use sync::util::ServerTimestamp;
use libc::c_char;


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
    pub server_modified: libc::c_double,
    pub payload_str: *mut c_char,
}

impl Drop for CleartextBsoC {
    fn drop(&mut self) {
        drop_and_null_c_str(&mut self.payload_str);
    }
}

impl CleartextBsoC {
    pub fn new(bso: &CleartextBso) -> Box<CleartextBsoC> {
        Box::new(CleartextBsoC {
            payload_str: string_to_c_str(bso.payload.clone().into_json_string()),
            server_modified: bso.modified.0,
        })
    }
}

#[no_mangle]
pub extern "C" fn sync15_service_create(
    key_id: *const libc::c_char ,
    access_token: *const libc::c_char ,
    sync_key: *const libc::c_char ,
    tokenserver_base_url: *const libc::c_char
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

#[no_mangle]
pub unsafe extern "C" fn sync15_service_destroy(svc: *mut Sync15Service) {
    Box::from_raw(svc);
}

/// Free a changeset previously returned by `sync15_changeset_create` or
/// `sync15_changeset_fetch`.
#[no_mangle]
pub unsafe extern "C" fn sync15_changeset_destroy(changeset: *mut RecordChangeset) {
    Box::from_raw(changeset);
}

/// Free a record previously returned by `sync15_changeset_get_record_at`.
#[no_mangle]
pub unsafe extern "C" fn sync15_record_destroy(bso: *mut CleartextBsoC) {
    Box::from_raw(bso);
}

/// Create a new outgoing changeset, which requires that the server have not been
/// modified since it returned the provided `timestamp`.
#[no_mangle]
pub extern "C" fn sync15_changeset_create(
    collection: *const c_char,
    timestamp: libc::c_double,
) -> *mut RecordChangeset {
    assert!(timestamp >= 0.0);
    Box::into_raw(Box::new(
        RecordChangeset::new(c_str_to_string(collection),
                             ServerTimestamp(timestamp))))
}

/// Get all the changes for the requested collection that have occurred since last_sync.
/// Important: Caller frees!
#[no_mangle]
pub extern "C" fn sync15_changeset_fetch(
    svc: *const Sync15Service,
    collection_c: *const c_char,
    last_sync: libc::c_double,
) -> *mut RecordChangeset {
    let service = unsafe { &*svc };
    let collection = c_str_to_string(collection_c);
    let result = RecordChangeset::fetch(service, collection, ServerTimestamp(last_sync as f64));
    let fetched = match result {
        Ok(r) => Box::new(r),
        Err(e) => {
            eprintln!("Unexpected error fetching collection: {}", e);
            return ptr::null_mut();
        }
    };
    Box::into_raw(fetched)
}

/// Get the last_sync timestamp for a (usually remote) changeset.
#[no_mangle]
pub extern "C" fn sync15_changeset_get_timestamp(changeset: *const RecordChangeset) -> libc::c_double {
    let changeset = unsafe { &*changeset };
    changeset.timestamp.0 as libc::c_double
}

/// Get the number of records from a (usually remote) changeset.
#[no_mangle]
pub extern "C" fn sync15_changeset_get_record_count(changeset: *const RecordChangeset) -> libc::size_t {
    let changeset = unsafe { &*changeset };
    changeset.changed.len()
}

/// Get the number of tombstones from a (usually remote) changeset.
#[no_mangle]
pub extern "C" fn sync15_changeset_get_tombstone_count(changeset: *const RecordChangeset) -> libc::size_t {
    let changeset = unsafe { &*changeset };
    changeset.deleted_ids.len()
}

/// Get the requested record from the (usually remote) changeset. `index` should be less
/// than `sync15_changeset_get_record_count`, or NULL will be returned and a message
/// logged to stderr.
///
/// Important: Caller needs to free the returned value using `sync15_record_destroy`
#[no_mangle]
pub extern "C" fn sync15_changeset_get_record_at(
    changeset: *const RecordChangeset,
    index: libc::size_t
) -> *mut CleartextBsoC {
    let changeset = unsafe { &*changeset };
    if index >= changeset.changed.len() {
        eprintln!("sync15_changeset_get_record_at was given an invalid index");
        return ptr::null_mut();
    }
    Box::into_raw(CleartextBsoC::new(&changeset.changed[index]))
}

/// Get the requested tombstone id from the (usually remote) changeset. `index`
/// should be less than `sync15_changeset_get_tombstone_count`, or NULL will be
/// returned an a message logged to stderr.
///
/// Important: Caller needs to free the returned string.
#[no_mangle]
pub extern "C" fn sync15_changeset_get_tombstone_at(
    changeset: *const RecordChangeset,
    index: libc::size_t
) -> *mut c_char {
    let changeset = unsafe { &*changeset };
    if index >= changeset.deleted_ids.len() {
        eprintln!("sync15_changeset_get_tombstone_at was given an invalid index");
        return ptr::null_mut();
    }
    string_to_c_str(changeset.deleted_ids[index].clone())
}

fn c_str_to_cleartext(json: *const c_char) -> sync::Result<Payload> {
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
pub extern "C" fn sync15_changeset_add_record(
    changeset: *mut RecordChangeset,
    record_json: *const c_char
) -> bool {
    let changeset = unsafe { &mut *changeset };
    assert!(!record_json.is_null());
    let cleartext = match c_str_to_cleartext(record_json) {
        Ok(ct) => ct,
        Err(e) => {
            eprintln!("Could not add record to changeset: {}", e);
            return false;
        }
    };
    // Arguably shouldn't support this and should have callers use add_tombstone, but w/e
    if cleartext.is_tombstone() {
        changeset.deleted_ids.push(cleartext.id);
    } else {
        let bso = cleartext.into_bso(changeset.collection.clone());
        changeset.changed.push(bso);
    }
    true
}

/// Add a tombstone to an outgoing changeset.
#[no_mangle]
pub extern "C" fn sync15_changeset_add_tombstone(changeset: *mut RecordChangeset,
                                                 record_id: *const c_char) {
    let changeset = unsafe { &mut *changeset };
    changeset.deleted_ids.push(c_str_to_string(record_id));
}
