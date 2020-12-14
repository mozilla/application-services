/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(unknown_lints)]
#![warn(rust_2018_idioms)]

use ffi_support::{
    define_bytebuffer_destructor, define_handle_map_deleter, define_string_destructor, ByteBuffer,
    ConcurrentHandleMap, ExternError, FfiStr,
};
use std::{
    os::raw::c_char,
    sync::{Arc, Mutex},
};
use tabs::{Result, TabsStore};

lazy_static::lazy_static! {
    // TODO: this is basically a RwLock<HandleMap<Mutex<Arc<Mutex<...>>>>.
    // but could just be a `RwLock<HandleMap<Arc<Mutex<...>>>>`.
    // Find a way to express this cleanly in ffi_support?
    pub static ref STORES: ConcurrentHandleMap<Arc<Mutex<TabsStore>>> = ConcurrentHandleMap::new();
}

fn parse_url(url: &str) -> Result<url::Url> {
    Ok(url::Url::parse(url)?)
}

#[no_mangle]
pub extern "C" fn remote_tabs_new(error: &mut ExternError) -> u64 {
    log::debug!("remote_tabs_new");
    STORES.insert_with_result(error, || -> Result<_> {
        Ok(Arc::new(Mutex::new(TabsStore::new())))
    })
}

#[no_mangle]
pub extern "C" fn remote_tabs_sync(
    handle: u64,
    key_id: FfiStr<'_>,
    access_token: FfiStr<'_>,
    sync_key: FfiStr<'_>,
    tokenserver_url: FfiStr<'_>,
    local_device_id: FfiStr<'_>,
    error: &mut ExternError,
) -> *mut c_char {
    log::debug!("remote_tabs_sync");
    STORES.call_with_result(error, handle, |store| -> Result<_> {
        let ping = store.lock().unwrap().sync(
            &sync15::Sync15StorageClientInit {
                key_id: key_id.into_string(),
                access_token: access_token.into_string(),
                tokenserver_url: parse_url(tokenserver_url.as_str())?,
            },
            &sync15::KeyBundle::from_ksync_base64(sync_key.as_str())?,
            local_device_id.as_str(),
        )?;
        Ok(ping)
    })
}

/// # Safety
/// Deref pointer, thus unsafe
#[no_mangle]
pub unsafe extern "C" fn remote_tabs_update_local(
    handle: u64,
    local_state: FfiStr<'_>,
    error: &mut ExternError,
) {
    log::debug!("remote_tabs_update_local");
    STORES.call_with_result(error, handle, |store| -> Result<_> {
        let remote_tabs = serde_json::from_str(local_state.as_str())?;
        store.lock().unwrap().update_local_state(remote_tabs);
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn remote_tabs_get_all(handle: u64, error: &mut ExternError) -> ByteBuffer {
    log::debug!("remote_tabs_get_all");
    use tabs::msg_types::ClientsTabs;
    STORES.call_with_result(error, handle, |store| -> Result<_> {
        Ok(store
            .lock()
            .unwrap()
            .remote_tabs()
            .map(|tabs| -> ClientsTabs { tabs.into() }))
    })
}

define_string_destructor!(remote_tabs_destroy_string);
define_bytebuffer_destructor!(remote_tabs_destroy_bytebuffer);
define_handle_map_deleter!(STORES, remote_tabs_destroy);
