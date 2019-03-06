/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use ffi_support::{
    define_bytebuffer_destructor, define_handle_map_deleter, define_string_destructor,
    ConcurrentHandleMap, ExternError,
};
use std::os::raw::c_char;
// use sync15::telemetry;

use base64;
use lazy_static;
use serde_json::{self, json};

use config::PushConfiguration;
use push_errors::{self, Result};
use subscriber::PushManager;

#[no_mangle]
pub extern "C" fn push_enable_logcat_logging() {
    #[cfg(target_os = "android")]
    {
        let _ = std::panic::catch_unwind(|| {
            android_logger::init_once(
                android_logger::Filter::default().with_min_level(log::Level::Debug),
                Some("libpush_ffi"),
            );
            log::debug!("Android logging should be hooked up!")
        });
    }
}

lazy_static::lazy_static! {
    static ref MANAGER: ConcurrentHandleMap<PushManager> = ConcurrentHandleMap::new();
}

/// Instantiate a Http connection. Returned connection must be freed with
/// `push_connection_destroy`. Returns null and logs on errors (for now).
#[no_mangle]
pub unsafe extern "C" fn push_connection_new(
    server_host: *const c_char,
    socket_protocol: *const c_char,
    bridge_type: *const c_char,
    registration_id: *const c_char,
    sender_id: *const c_char,
    database_path: *const c_char,
    error: &mut ExternError,
) -> u64 {
    MANAGER.insert_with_result(error, || {
        log::debug!(
            "push_connection_new {:?} {:?} -> {:?} {:?}=>{:?}",
            socket_protocol,
            server_host,
            bridge_type,
            sender_id,
            registration_id
        );
        // return this as a reference to the map since that map contains the actual handles that rust uses.
        // see ffi layer for details.
        let host = ffi_support::rust_string_from_c(server_host);
        let protocol = ffi_support::opt_rust_string_from_c(socket_protocol);
        let reg_id = ffi_support::opt_rust_string_from_c(registration_id);
        let bridge = ffi_support::opt_rust_string_from_c(bridge_type);
        let sender = ffi_support::rust_string_from_c(sender_id);
        let db_path = ffi_support::opt_rust_string_from_c(database_path);
        let config = PushConfiguration {
            server_host: host,
            http_protocol: protocol,
            bridge_type: bridge,
            registration_id: reg_id,
            sender_id: sender,
            database_path: db_path,
            ..Default::default()
        };
        PushManager::new(config.clone())
    })
}

// Add a subscription
/// Errors are logged.
#[no_mangle]
pub unsafe extern "C" fn push_subscribe(
    handle: u64,
    channel_id: *const c_char,
    scope: *const c_char,
    error: &mut ExternError,
) -> *mut c_char {
    log::debug!("push_get_subscription");
    MANAGER.call_with_result_mut(error, handle, |mgr| -> Result<String> {
        let channel = ffi_support::rust_str_from_c(channel_id);
        let scope_s = ffi_support::rust_str_from_c(scope);
        // Don't auto add the subscription to the db.
        // (endpoint updates also call subscribe and should be lighter weight)
        let (info, subscription_key) = mgr.subscribe(channel, scope_s)?;
        // store the channelid => auth + subscription_key
        let subscription_info = json!({
            "endpoint": info.endpoint,
            "keys": {
                "auth": base64::encode_config(&subscription_key.auth,
                                              base64::URL_SAFE_NO_PAD),
                "p256dh": base64::encode_config(&subscription_key.public,
                                                base64::URL_SAFE_NO_PAD)
            }
        });
        return Ok(subscription_info.to_string());
    })
}

// Unsubscribe a channel
#[no_mangle]
pub unsafe extern "C" fn push_unsubscribe(
    handle: u64,
    channel_id: *const c_char,
    error: &mut ExternError,
) -> u8 {
    log::debug!("push_unsubscribe");
    MANAGER.call_with_result_mut(error, handle, |mgr| -> Result<bool> {
        let channel = ffi_support::opt_rust_str_from_c(channel_id);
        mgr.unsubscribe(channel)
    })
}

// Update the OS token
#[no_mangle]
pub unsafe extern "C" fn push_update(
    handle: u64,
    new_token: *const c_char,
    error: &mut ExternError,
) -> u8 {
    log::debug!("push_update");
    MANAGER.call_with_result_mut(error, handle, |mgr| -> Result<_> {
        let token = ffi_support::rust_str_from_c(new_token);
        mgr.update(&token)
    })
}

// verify connection using channel list
// Returns a JSON containing the new channelids => endpoints
// NOTE: AC should notify processes associated with channelIDs of new endpoint
#[no_mangle]
pub unsafe extern "C" fn push_verify_connection(
    handle: u64,
    error: &mut ExternError,
) -> *mut c_char {
    log::debug!("push_verify");
    MANAGER.call_with_result_mut(error, handle, |mgr| -> Result<_> {
        if let Ok(r) = mgr.verify_connection() {
            if r == false {
                if let Ok(new_endpoints) = mgr.regenerate_endpoints() {
                    // use a `match` here to resolve return of <_>
                    return serde_json::to_string(&new_endpoints).map_err(|e| {
                        push_errors::ErrorKind::TranscodingError(format!("{:?}", e)).into()
                    });
                }
            }
        }
        Ok(String::from(""))
    })
}

#[no_mangle]
pub unsafe extern "C" fn push_decrypt(
    handle: u64,
    chid: *const c_char,
    body: *const c_char,
    encoding: *const c_char,
    salt: *const c_char,
    dh: *const c_char,
    error: &mut ExternError,
) -> *mut c_char {
    log::debug!("push_decrypt");
    MANAGER.call_with_result_mut(error, handle, |mgr| {
        let r_chid = ffi_support::rust_str_from_c(chid);
        let r_body = ffi_support::rust_str_from_c(body);
        let r_encoding = ffi_support::rust_str_from_c(encoding);
        let r_salt: Option<&str> = ffi_support::opt_rust_str_from_c(salt);
        let r_dh: Option<&str> = ffi_support::opt_rust_str_from_c(dh);
        let uaid = mgr.conn.uaid.clone().unwrap();
        mgr.decrypt(&uaid, r_chid, r_body, r_encoding, r_dh, r_salt)
    })
}
// TODO: modify these to be relevant.

define_string_destructor!(push_destroy_string);
define_bytebuffer_destructor!(push_destroy_buffer);
define_handle_map_deleter!(MANAGER, push_connection_destroy);
// define_box_destructor!(PlacesInterruptHandle, places_interrupt_handle_destroy);
