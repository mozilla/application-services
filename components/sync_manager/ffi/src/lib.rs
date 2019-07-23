/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(unknown_lints)]
#![warn(rust_2018_idioms)]
// Let's allow these in the FFI code, since it's usually just a coincidence if
// the closure is small.
#![allow(clippy::redundant_closure)]

use ffi_support::{ExternError, HandleError};
use sync_manager::Result as MgrResult;

#[no_mangle]
pub extern "C" fn sync_manager_set_places(_places_api_handle: u64, error: &mut ExternError) {
    ffi_support::call_with_result(error, || -> MgrResult<()> {
        #[cfg(feature = "places")]
        {
            let api = places_ffi::APIS
                .get_u64(_places_api_handle, |api| -> Result<_, HandleError> {
                    Ok(std::sync::Arc::clone(api))
                })?;
            sync_manager::set_places(api);
            Ok(())
        }
        #[cfg(not(feature = "places"))]
        {
            log::error!("Sync manager not compiled with places support");
            Err(sync_manager::ErrorKind::UnsupportedFeature("places".to_string()).into())
        }
    })
}

#[no_mangle]
pub extern "C" fn sync_manager_set_logins(_logins_handle: u64, error: &mut ExternError) {
    ffi_support::call_with_result(error, || -> MgrResult<()> {
        #[cfg(feature = "logins")]
        {
            let api = logins_ffi::ENGINES
                .get_u64(_logins_handle, |api| -> Result<_, HandleError> {
                    Ok(std::sync::Arc::clone(api))
                })?;
            sync_manager::set_logins(api);
            Ok(())
        }
        #[cfg(not(feature = "logins"))]
        {
            log::error!("Sync manager not compiled with logins support");
            Err(sync_manager::ErrorKind::UnsupportedFeature("logins".to_string()).into())
        }
    })
}

#[no_mangle]
pub extern "C" fn sync_manager_disconnect(error: &mut ExternError) {
    ffi_support::call_with_output(error, sync_manager::disconnect);
}

unsafe fn get_buffer<'a>(data: *const u8, len: i32) -> &'a [u8] {
    assert!(len >= 0, "Bad buffer len: {}", len);
    if len == 0 {
        // This will still fail, but as a bad protobuf format.
        &[]
    } else {
        assert!(!data.is_null(), "Unexpected null data pointer");
        std::slice::from_raw_parts(data, len as usize)
    }
}

#[no_mangle]
pub unsafe extern "C" fn sync_manager_sync(
    params_data: *const u8,
    params_len: i32,
    error: &mut ExternError,
) -> ffi_support::ByteBuffer {
    ffi_support::call_with_result(error, || {
        let buffer = get_buffer(params_data, params_len);
        let params: sync_manager::msg_types::SyncParams = prost::Message::decode(buffer)?;
        sync_manager::sync(params)
    })
}

ffi_support::define_string_destructor!(sync_manager_destroy_string);
ffi_support::define_bytebuffer_destructor!(sync_manager_destroy_bytebuffer);
