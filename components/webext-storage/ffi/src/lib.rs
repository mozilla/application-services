/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::os::raw::c_char;

use ffi_support::{define_handle_map_deleter, ConcurrentHandleMap, ExternError, FfiStr};
use webext_storage::{error, store::WebExtStorageStore as Store};

lazy_static::lazy_static! {
    static ref STORES: ConcurrentHandleMap<Store> = ConcurrentHandleMap::new();
}

#[no_mangle]
pub extern "C" fn webext_store_new(db_path: FfiStr<'_>, error: &mut ExternError) -> u64 {
    log::debug!("webext_store_new");
    STORES.insert_with_result(error, || -> error::Result<Store> {
        let path = db_path.as_str();
        Store::new(path)
    })
}

#[no_mangle]
pub extern "C" fn webext_store_set(
    handle: u64,
    ext_id: FfiStr<'_>,
    json: FfiStr<'_>,
    error: &mut ExternError,
) -> *mut c_char {
    log::debug!("webext_store_set");
    STORES.call_with_result(error, handle, |store| -> error::Result<_> {
        let val = serde_json::from_str(json.as_str())?;
        let changes = store.set(ext_id.as_str(), val)?;
        Ok(serde_json::to_string(&changes)?)
    })
}

#[no_mangle]
pub extern "C" fn webext_store_get(
    handle: u64,
    ext_id: FfiStr<'_>,
    keys: FfiStr<'_>,
    error: &mut ExternError,
) -> *mut c_char {
    log::debug!("webext_store_get");
    STORES.call_with_result(error, handle, |store| -> error::Result<_> {
        let keys = serde_json::from_str(keys.as_str())?;
        let val = store.get(ext_id.as_str(), keys)?;
        Ok(serde_json::to_string(&val)?)
    })
}

#[no_mangle]
pub extern "C" fn webext_store_remove(
    handle: u64,
    ext_id: FfiStr<'_>,
    keys: FfiStr<'_>,
    error: &mut ExternError,
) -> *mut c_char {
    log::debug!("webext_store_remove");
    STORES.call_with_result(error, handle, |store| -> error::Result<_> {
        let keys = serde_json::from_str(keys.as_str())?;
        let changes = store.remove(ext_id.as_str(), keys)?;
        Ok(serde_json::to_string(&changes)?)
    })
}

#[no_mangle]
pub extern "C" fn webext_store_clear(
    handle: u64,
    ext_id: FfiStr<'_>,
    error: &mut ExternError,
) -> *mut c_char {
    log::debug!("webext_store_clear");
    STORES.call_with_result(error, handle, |store| -> error::Result<_> {
        let changes = store.clear(ext_id.as_str())?;
        Ok(serde_json::to_string(&changes)?)
    })
}

// For the FFI, we rely on `ffi-support` to generate a deleter for us, which
// automatically closes the underlying database connection when the store is
// dropped. Since the deleter catches panics, we don't need to use
// `Store::teardown` (unlike on Desktop, where panicking aborts). Note that,
// if `webext_store_destroy` fails, the connection will still be leaked.
define_handle_map_deleter!(STORES, webext_store_destroy);
