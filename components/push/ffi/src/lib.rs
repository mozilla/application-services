/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(unknown_lints)]
#![warn(rust_2018_idioms)]

use ffi_support::{
    define_bytebuffer_destructor, define_handle_map_deleter, define_string_destructor,
    ConcurrentHandleMap, ExternError, FfiStr,
};
use std::os::raw::c_char;

use serde_json::{self, json};

use push::config::PushConfiguration;
use push::error::Result;
use push::subscriber::PushManager;

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
pub extern "C" fn push_connection_new(
    server_host: FfiStr<'_>,
    http_protocol: FfiStr<'_>,
    bridge_type: FfiStr<'_>,
    registration_id: FfiStr<'_>,
    sender_id: FfiStr<'_>,
    database_path: FfiStr<'_>,
    error: &mut ExternError,
) -> u64 {
    MANAGER.insert_with_result(error, || {
        log::trace!(
            "push_connection_new {:?} {:?} -> {:?} {:?}=>{:?}",
            http_protocol,
            server_host,
            bridge_type,
            sender_id,
            registration_id
        );
        // return this as a reference to the map since that map contains the actual handles that rust uses.
        // see ffi layer for details.
        let host = server_host.into_string();
        let protocol = http_protocol.into_opt_string();
        let reg_id = registration_id.into_opt_string();
        let bridge = bridge_type.into_opt_string();
        let sender = sender_id.into_string();
        let db_path = database_path.into_opt_string();
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
pub extern "C" fn push_subscribe(
    handle: u64,
    channel_id: FfiStr<'_>,
    scope: FfiStr<'_>,
    error: &mut ExternError,
) -> *mut c_char {
    log::debug!("push_get_subscription");
    MANAGER.call_with_result_mut(error, handle, |mgr| -> Result<String> {
        let channel = channel_id.as_str();
        let scope_s = scope.as_str();
        // Don't auto add the subscription to the db.
        // (endpoint updates also call subscribe and should be lighter weight)
        let (info, subscription_key) = mgr.subscribe(channel, scope_s)?;
        // it is possible for the
        // store the channel_id => auth + subscription_key
        let subscription_response = json!({
            "channel_id": info.channel_id,
            "subscription_info": {
                "endpoint": info.endpoint,
                "keys": {
                    "auth": base64::encode_config(&subscription_key.auth,
                                                  base64::URL_SAFE_NO_PAD),
                    "p256dh": base64::encode_config(&subscription_key.public,
                                                    base64::URL_SAFE_NO_PAD)
                }
            }
        });
        Ok(subscription_response.to_string())
    })
}

// Unsubscribe a channel
#[no_mangle]
pub extern "C" fn push_unsubscribe(
    handle: u64,
    channel_id: FfiStr<'_>,
    error: &mut ExternError,
) -> u8 {
    log::debug!("push_unsubscribe");
    MANAGER.call_with_result_mut(error, handle, |mgr| -> Result<bool> {
        let channel = channel_id.as_opt_str();
        mgr.unsubscribe(channel)
    })
}

// Update the OS token
#[no_mangle]
pub extern "C" fn push_update(handle: u64, new_token: FfiStr<'_>, error: &mut ExternError) -> u8 {
    log::debug!("push_update");
    MANAGER.call_with_result_mut(error, handle, |mgr| -> Result<_> {
        let token = new_token.as_str();
        mgr.update(&token)
    })
}

// verify connection using channel list
// Returns a JSON containing the new channel_ids => endpoints
// NOTE: AC should notify processes associated with channel_ids of new endpoint
#[no_mangle]
pub extern "C" fn push_verify_connection(handle: u64, error: &mut ExternError) -> *mut c_char {
    log::debug!("push_verify");
    MANAGER.call_with_result_mut(error, handle, |mgr| -> Result<_> {
        if !mgr.verify_connection()? {
            let new_endpoints = mgr.regenerate_endpoints()?;
            if !new_endpoints.is_empty() {
                return serde_json::to_string(&new_endpoints).map_err(|e| {
                    push::error::ErrorKind::TranscodingError(format!("{:?}", e)).into()
                });
            }
        }
        Ok(String::from(""))
    })
}

#[no_mangle]
pub extern "C" fn push_decrypt(
    handle: u64,
    chid: FfiStr<'_>,
    body: FfiStr<'_>,
    encoding: FfiStr<'_>,
    salt: FfiStr<'_>,
    dh: FfiStr<'_>,
    error: &mut ExternError,
) -> *mut c_char {
    log::debug!("push_decrypt");
    MANAGER.call_with_result_mut(error, handle, |mgr| {
        let r_chid = chid.as_str();
        let r_body = body.as_str();
        let r_encoding = encoding.as_str();
        let r_salt: Option<&str> = salt.as_opt_str();
        let r_dh: Option<&str> = dh.as_opt_str();
        let uaid = mgr.conn.uaid.clone().unwrap();
        mgr.decrypt(&uaid, r_chid, r_body, r_encoding, r_dh, r_salt)
    })
}
// TODO: modify these to be relevant.

#[no_mangle]
pub extern "C" fn push_dispatch_for_chid(
    handle: u64,
    chid: FfiStr<'_>,
    error: &mut ExternError,
) -> *mut c_char {
    log::debug!("push_dispatch_for_chid");
    MANAGER.call_with_result_mut(error, handle, |mgr| -> Result<String> {
        let chid = chid.as_str();
        match mgr.get_record_by_chid(chid)? {
            Some(record) => {
                let dispatch = json!({
                    "uaid": record.uaid,
                    "scope": record.scope,
                });
                Ok(dispatch.to_string())
            }
            None => Ok(String::from("")),
        }
    })
}

define_string_destructor!(push_destroy_string);
define_bytebuffer_destructor!(push_destroy_buffer);
define_handle_map_deleter!(MANAGER, push_connection_destroy);
