/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(unknown_lints)]
#![warn(rust_2018_idioms)]
// Let's allow these in the FFI code, since it's usually just a coincidence if
// the closure is small.
#![allow(clippy::redundant_closure)]
use accounts::{DeviceConfig, FxAccountManager};
use ffi_support::{
    define_bytebuffer_destructor, define_handle_map_deleter, define_string_destructor, ByteBuffer,
    ConcurrentHandleMap, ExternError, FfiStr,
};
use fxa_client::{
    device::{Capability as DeviceCapability, PushSubscription},
    msg_types as fxa_msg_types, Config as RemoteConfig,
};
use std::os::raw::c_char;

lazy_static::lazy_static! {
    static ref MANAGER: ConcurrentHandleMap<FxAccountManager> = ConcurrentHandleMap::new();
}

/// Creates an [FxAccountManager].
///
/// # Safety
///
/// A destructor [fxa_mgr_free] is provided for releasing the memory for this
/// pointer type.
#[no_mangle]
pub extern "C" fn fxa_mgr_new(
    // TODO: We should just use protobufs structs here instead of 7 args!
    content_url: FfiStr<'_>,
    client_id: FfiStr<'_>,
    redirect_uri: FfiStr<'_>,
    device_name: FfiStr<'_>,
    device_type: i32,
    capabilities_data: *const u8,
    capabilities_len: i32,
    err: &mut ExternError,
) -> u64 {
    log::debug!("fxa_mgr_new");
    MANAGER.insert_with_output(err, || {
        let content_url = content_url.as_str();
        let client_id = client_id.as_str();
        let redirect_uri = redirect_uri.as_str();
        let device_name = device_name.as_str();
        let capabilities = unsafe {
            DeviceCapability::from_protobuf_array_ptr(capabilities_data, capabilities_len)
        };
        let device_type =
            fxa_msg_types::device::Type::from_i32(device_type).expect("Unknown device type code");
        let remote_config = RemoteConfig::new(content_url, client_id, redirect_uri);
        let device_config = DeviceConfig::new(device_name, device_type.into(), capabilities);
        FxAccountManager::new(remote_config, device_config)
    })
}

/// TODO
#[no_mangle]
pub extern "C" fn fxa_mgr_init(handle: u64, json_state: FfiStr<'_>, error: &mut ExternError) {
    log::debug!("fxa_mgr_init");
    let json_state = json_state.as_opt_str();
    MANAGER.call_with_output_mut(error, handle, |mgr| mgr.init(json_state))
}

/// Request a OAuth token by starting a new OAuth flow.
///
/// This function returns a URL string that the caller should open in a webview.
///
/// Once the user has confirmed the authorization grant, they will get redirected to `redirect_url`:
/// the caller must intercept that redirection, extract the `code` and `state` query parameters and call
/// [fxa_complete_oauth_flow] to complete the flow.
///
/// # Safety
///
/// A destructor [fxa_mgr_str_free] is provided for releasing the memory for this
/// pointer type.
#[no_mangle]
pub extern "C" fn fxa_mgr_begin_oauth_flow(handle: u64, error: &mut ExternError) -> *mut c_char {
    log::debug!("fxa_mgr_begin_oauth_flow");
    MANAGER.call_with_result_mut(error, handle, |mgr| mgr.begin_oauth_flow())
}

/// Request a OAuth token by starting a new pairing flow, by calling the content server pairing endpoint.
///
/// This function returns a URL string that the caller should open in a webview.
///
/// Pairing assumes you want keys by default, so you must provide a key-bearing scope.
///
/// # Safety
///
/// A destructor [fxa_mgr_str_free] is provided for releasing the memory for this
/// pointer type.
#[no_mangle]
pub extern "C" fn fxa_mgr_begin_pairing_flow(
    handle: u64,
    pairing_url: FfiStr<'_>,
    error: &mut ExternError,
) -> *mut c_char {
    log::debug!("fxa_mgr_begin_pairing_flow");
    MANAGER.call_with_result_mut(error, handle, |mgr| {
        let pairing_url = pairing_url.as_str();
        mgr.begin_pairing_flow(&pairing_url)
    })
}

/// Finish an OAuth flow initiated by [fxa_begin_oauth_flow].
#[no_mangle]
pub extern "C" fn fxa_mgr_finish_authentication_flow(
    handle: u64,
    code: FfiStr<'_>,
    state: FfiStr<'_>,
    error: &mut ExternError,
) {
    log::debug!("fxa_mgr_finish_authentication_flow");
    MANAGER.call_with_output_mut(error, handle, |mgr| {
        let code = code.as_str();
        let state = state.as_str();
        mgr.finish_authentication_flow(code, state)
    });
}

/// TODO
#[no_mangle]
pub extern "C" fn fxa_mgr_on_authentication_error(handle: u64, error: &mut ExternError) {
    log::debug!("fxa_mgr_on_authentication_error");
    MANAGER.call_with_output_mut(error, handle, |mgr| mgr.on_authentication_error())
}

/// TODO
#[no_mangle]
pub extern "C" fn fxa_mgr_logout(handle: u64, error: &mut ExternError) {
    log::debug!("fxa_mgr_logout");
    MANAGER.call_with_output_mut(error, handle, |mgr| mgr.logout())
}

/// TODO
#[no_mangle]
pub extern "C" fn fxa_mgr_get_profile(handle: u64, error: &mut ExternError) -> ByteBuffer {
    log::debug!("fxa_mgr_get_profile");
    MANAGER.call_with_output_mut(error, handle, |mgr| {
        mgr.get_profile()
            .map(|p| -> Option<fxa_client::msg_types::Profile> { Some(p.into()) })
    })
}

/// TODO
#[no_mangle]
pub extern "C" fn fxa_mgr_update_profile(handle: u64, error: &mut ExternError) -> ByteBuffer {
    log::debug!("fxa_mgr_update_profile");
    MANAGER.call_with_output_mut(error, handle, |mgr| {
        mgr.update_profile()
            .map(|p| -> Option<fxa_client::msg_types::Profile> { Some(p.into()) })
    })
}

/// TODO
#[no_mangle]
pub extern "C" fn fxa_mgr_account_state(handle: u64, error: &mut ExternError) -> ByteBuffer {
    log::debug!("fxa_mgr_account_state");
    MANAGER.call_with_output_mut(error, handle, |mgr| mgr.account_state())
}

/// TODO
#[no_mangle]
pub extern "C" fn fxa_mgr_export_persisted_state(
    handle: u64,
    error: &mut ExternError,
) -> *mut c_char {
    log::debug!("fxa_mgr_export_persisted_state");
    MANAGER.call_with_result_mut(error, handle, |mgr| mgr.export_persisted_state())
}

/// TODO
#[no_mangle]
pub extern "C" fn fxa_mgr_update_devices(handle: u64, error: &mut ExternError) -> ByteBuffer {
    log::debug!("fxa_mgr_update_devices");
    MANAGER.call_with_output_mut(error, handle, |mgr| mgr.update_devices())
}

/// TODO
#[no_mangle]
pub extern "C" fn fxa_mgr_get_devices(handle: u64, error: &mut ExternError) -> ByteBuffer {
    log::debug!("fxa_mgr_get_devices");
    MANAGER.call_with_output_mut(error, handle, |mgr| mgr.get_devices())
}

// TODO
#[no_mangle]
pub extern "C" fn fxa_mgr_handle_push_message(
    handle: u64,
    json_payload: FfiStr<'_>,
    error: &mut ExternError,
) -> ByteBuffer {
    log::debug!("fxa_mgr_handle_push_message");
    MANAGER.call_with_result_mut(error, handle, |mgr| {
        mgr.get_account()
            .handle_push_message(json_payload.as_str())
            .map(|evs| {
                let events = evs.into_iter().map(|e| e.into()).collect();
                fxa_client::msg_types::AccountEvents { events }
            })
    })
}

// TODO
#[no_mangle]
pub extern "C" fn fxa_mgr_set_device_name(
    handle: u64,
    display_name: FfiStr<'_>,
    error: &mut ExternError,
) {
    log::debug!("fxa_mgr_set_device_name");
    MANAGER.call_with_result_mut(error, handle, |mgr| {
        // We don't really care about passing back the resulting Device record.
        // We might in the future though.
        mgr.get_account()
            .set_device_name(display_name.as_str())
            .map(|_| ())
    })
}

// TODO
#[no_mangle]
pub extern "C" fn fxa_mgr_set_push_subscription(
    handle: u64,
    endpoint: FfiStr<'_>,
    public_key: FfiStr<'_>,
    auth_key: FfiStr<'_>,
    error: &mut ExternError,
) {
    log::debug!("fxa_mgr_set_push_subscription");
    MANAGER.call_with_result_mut(error, handle, |mgr| {
        let ps = PushSubscription {
            endpoint: endpoint.into_string(),
            public_key: public_key.into_string(),
            auth_key: auth_key.into_string(),
        };
        // We don't really care about passing back the resulting Device record.
        // We might in the future though.
        mgr.get_account().set_push_subscription(&ps).map(|_| ())
    })
}

// TODO
#[no_mangle]
pub extern "C" fn fxa_mgr_send_tab(
    handle: u64,
    target_device_id: FfiStr<'_>,
    title: FfiStr<'_>,
    url: FfiStr<'_>,
    error: &mut ExternError,
) {
    log::debug!("fxa_mgr_send_tab");
    let target = target_device_id.as_str();
    let title = title.as_str();
    let url = url.as_str();
    MANAGER.call_with_result_mut(error, handle, |mgr| {
        mgr.get_account().send_tab(target, title, url)
    })
}

// TODO
#[no_mangle]
pub extern "C" fn fxa_mgr_poll_device_commands(handle: u64, error: &mut ExternError) -> ByteBuffer {
    log::debug!("fxa_mgr_poll_device_commands");
    MANAGER.call_with_result_mut(error, handle, |mgr| {
        mgr.get_account().poll_device_commands().map(|evs| {
            let events = evs.into_iter().map(|e| e.into()).collect();
            fxa_client::msg_types::AccountEvents { events }
        })
    })
}

define_handle_map_deleter!(MANAGER, fxa_mgr_free);
define_string_destructor!(fxa_mgr_str_free);
define_bytebuffer_destructor!(fxa_mgr_bytebuffer_free);
