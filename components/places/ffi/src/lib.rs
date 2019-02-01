/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use ffi_support::{
    define_box_destructor, define_handle_map_deleter, define_string_destructor, rust_str_from_c,
    rust_string_from_c, ConcurrentHandleMap, ExternError,
};
use places::history_sync::store::HistoryStore;
use places::{db::PlacesInterruptHandle, storage, PlacesDb};
use sync15::telemetry;

use std::os::raw::c_char;

use places::api::matcher::{match_url, search_frecent, SearchParams};

// indirection to help `?` figure out the target error type
fn parse_url(url: &str) -> sync15::Result<url::Url> {
    Ok(url::Url::parse(url)?)
}

#[no_mangle]
pub extern "C" fn places_enable_logcat_logging() {
    #[cfg(target_os = "android")]
    {
        let _ = std::panic::catch_unwind(|| {
            android_logger::init_once(
                android_logger::Filter::default().with_min_level(log::Level::Debug),
                Some("libplaces_ffi"),
            );
            log::debug!("Android logging should be hooked up!")
        });
    }
}

lazy_static::lazy_static! {
    static ref CONNECTIONS: ConcurrentHandleMap<PlacesDb> = ConcurrentHandleMap::new();
}

/// Instantiate a places connection. Returned connection must be freed with
/// `places_connection_destroy`. Returns null and logs on errors (for now).
#[no_mangle]
pub unsafe extern "C" fn places_connection_new(
    db_path: *const c_char,
    encryption_key: *const c_char,
    error: &mut ExternError,
) -> u64 {
    log::debug!("places_connection_new");
    CONNECTIONS.insert_with_result(error, || {
        let path = ffi_support::rust_string_from_c(db_path);
        let key = ffi_support::opt_rust_string_from_c(encryption_key);
        PlacesDb::open(path, key.as_ref().map(|v| v.as_str()))
    })
}

/// Get the interrupt handle for a connection. Must be destroyed with
/// `places_interrupt_handle_destroy`.
#[no_mangle]
pub extern "C" fn places_new_interrupt_handle(
    handle: u64,
    error: &mut ExternError,
) -> *mut PlacesInterruptHandle {
    CONNECTIONS.call_with_output(error, handle, |conn| conn.new_interrupt_handle())
}

#[no_mangle]
pub extern "C" fn places_interrupt(handle: &PlacesInterruptHandle, error: &mut ExternError) {
    ffi_support::call_with_output(error, || handle.interrupt())
}

/// Add an observation to the database. The observation is a VisitObservation represented as JSON.
/// Errors are logged.
#[no_mangle]
pub unsafe extern "C" fn places_note_observation(
    handle: u64,
    json_observation: *const c_char,
    error: &mut ExternError,
) {
    log::debug!("places_note_observation");
    CONNECTIONS.call_with_result_mut(error, handle, |conn| {
        let json = ffi_support::rust_str_from_c(json_observation);
        let visit: places::VisitObservation = serde_json::from_str(&json)?;
        places::api::apply_observation(conn, visit)
    })
}

/// Execute a query, returning a `Vec<SearchResult>` as a JSON string. Returned string must be freed
/// using `places_destroy_string`. Returns null and logs on errors (for now).
#[no_mangle]
pub unsafe extern "C" fn places_query_autocomplete(
    handle: u64,
    search: *const c_char,
    limit: u32,
    error: &mut ExternError,
) -> *mut c_char {
    log::debug!("places_query_autocomplete");
    CONNECTIONS.call_with_result(error, handle, |conn| {
        search_frecent(
            conn,
            SearchParams {
                search_string: ffi_support::rust_string_from_c(search),
                limit,
            },
        )
    })
}

/// Execute a query, returning a URL string or null. Returned string must be freed
/// using `places_destroy_string`. Returns null if no match is found.
#[no_mangle]
pub unsafe extern "C" fn places_match_url(
    handle: u64,
    search: *const c_char,
    error: &mut ExternError,
) -> *mut c_char {
    log::debug!("places_match_url");
    CONNECTIONS.call_with_result(error, handle, |conn| {
        match_url(conn, ffi_support::rust_string_from_c(search))
    })
}

#[no_mangle]
pub unsafe extern "C" fn places_get_visited(
    handle: u64,
    urls: *const *const c_char,
    urls_len: i32,
    byte_buffer: *mut bool,
    byte_buffer_len: i32,
    error: &mut ExternError,
) {
    log::debug!("places_get_visited");
    // This function has a dumb amount of overhead and copying...
    CONNECTIONS.call_with_result(error, handle, |conn| -> places::Result<()> {
        assert!(
            urls_len >= 0,
            "Negative array length provided to places_get_visited {}",
            urls_len
        );
        assert_eq!(byte_buffer_len, urls_len);
        let url_ptrs = std::slice::from_raw_parts(urls, urls_len as usize);
        let output = std::slice::from_raw_parts_mut(byte_buffer, byte_buffer_len as usize);
        let urls = url_ptrs
            .iter()
            .enumerate()
            .filter_map(|(idx, &p)| {
                let s = ffi_support::rust_str_from_c(p);
                url::Url::parse(s).ok().map(|url| (idx, url))
            })
            .collect::<Vec<_>>();
        storage::history::get_visited_into(conn, &urls, output)?;
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn places_get_visited_urls_in_range(
    handle: u64,
    start: i64,
    end: i64,
    include_remote: u8, // JNA has issues with bools...
    error: &mut ExternError,
) -> *mut c_char {
    log::debug!("places_get_visited_in_range");
    CONNECTIONS.call_with_result(error, handle, |conn| -> places::Result<_> {
        let visited = storage::history::get_visited_urls(
            conn,
            // Probably should allow into()...
            places::Timestamp(start.max(0) as u64),
            places::Timestamp(end.max(0) as u64),
            include_remote != 0,
        )?;
        Ok(serde_json::to_string(&visited)?)
    })
}

#[no_mangle]
pub unsafe extern "C" fn sync15_history_sync(
    handle: u64,
    key_id: *const c_char,
    access_token: *const c_char,
    sync_key: *const c_char,
    tokenserver_url: *const c_char,
    error: &mut ExternError,
) {
    log::debug!("sync15_history_sync");
    CONNECTIONS.call_with_result(error, handle, |conn| -> places::Result<_> {
        // XXX - this is wrong - we kinda want this to be long-lived - the "Db"
        // should own the store, but it's not part of the db.
        let store = HistoryStore::new(conn);
        let mut sync_ping = telemetry::SyncTelemetryPing::new();
        let result = store.sync(
            &sync15::Sync15StorageClientInit {
                key_id: rust_string_from_c(key_id),
                access_token: rust_string_from_c(access_token),
                tokenserver_url: parse_url(rust_str_from_c(tokenserver_url))?,
            },
            &sync15::KeyBundle::from_ksync_base64(rust_str_from_c(sync_key))?,
            &mut sync_ping,
        );
        result
    })
}

define_string_destructor!(places_destroy_string);
define_handle_map_deleter!(CONNECTIONS, places_connection_destroy);
define_box_destructor!(PlacesInterruptHandle, places_interrupt_handle_destroy);
