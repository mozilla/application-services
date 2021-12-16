/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(unknown_lints)]
#![warn(rust_2018_idioms)]
// Let's allow these in the FFI code, since it's usually just a coincidence if
// the closure is small.
#![allow(clippy::redundant_closure)]

use ffi_support::{
    define_box_destructor, define_bytebuffer_destructor, define_handle_map_deleter,
    define_string_destructor, ExternError, FfiStr,
};
use places::error::*;
pub use places::ffi::{APIS, CONNECTIONS};
use places::{ConnectionType, PlacesApi};
use sql_support::SqlInterruptHandle;

/// Instantiate a places API. Returned api must be freed with
/// `places_api_destroy`. Returns null and logs on errors (for now).
#[no_mangle]
pub extern "C" fn places_api_new(db_path: FfiStr<'_>, error: &mut ExternError) -> u64 {
    log::debug!("places_api_new");
    APIS.insert_with_result(error, || {
        let path = db_path.as_str();
        PlacesApi::new(path)
    })
}

#[no_mangle]
pub extern "C" fn places_connection_new(
    handle: u64,
    conn_type_val: u8,
    error: &mut ExternError,
) -> u64 {
    log::debug!("places_connection_new");
    APIS.call_with_result(error, handle, |api| -> places::Result<_> {
        let conn_type = match ConnectionType::from_primitive(conn_type_val) {
            // You can't open a sync connection using this method.
            None | Some(ConnectionType::Sync) => {
                return Err(ErrorKind::InvalidConnectionType.into());
            }
            Some(val) => val,
        };
        Ok(CONNECTIONS.insert(api.open_connection(conn_type)?))
    })
}

// Best effort, ignores failure.
#[no_mangle]
pub extern "C" fn places_api_return_write_conn(
    api_handle: u64,
    write_handle: u64,
    error: &mut ExternError,
) {
    log::debug!("places_api_return_write_conn");
    APIS.call_with_result(error, api_handle, |api| -> places::Result<_> {
        let write_conn = if let Ok(Some(conn)) = CONNECTIONS.remove_u64(write_handle) {
            conn
        } else {
            log::warn!("Can't return connection to PlacesApi because it does not exist");
            return Ok(());
        };
        if let Err(e) = api.close_connection(write_conn) {
            log::warn!("Failed to close connection: {}", e);
        }
        Ok(())
    })
}

define_string_destructor!(places_destroy_string);
define_bytebuffer_destructor!(places_destroy_bytebuffer);
define_handle_map_deleter!(APIS, places_api_destroy);

define_handle_map_deleter!(CONNECTIONS, places_connection_destroy);
define_box_destructor!(SqlInterruptHandle, places_interrupt_handle_destroy);
