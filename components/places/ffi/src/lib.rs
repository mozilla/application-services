/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

extern crate serde_json;
extern crate rusqlite;
extern crate places;
extern crate url;

#[macro_use]
extern crate log;

#[cfg(target_os = "android")]
extern crate android_logger;

#[macro_use]
extern crate ffi_support;

use std::os::raw::c_char;
use places::{storage, PlacesDb};
use ffi_support::{call_with_result, ExternError};

use places::api::matcher::{
    search_frecent,
    SearchParams,
};

fn logging_init() {
    #[cfg(target_os = "android")]
    {
        android_logger::init_once(
            android_logger::Filter::default().with_min_level(log::Level::Trace),
            Some("libplaces_ffi"));
        debug!("Android logging should be hooked up!")
    }
}

// XXX I'm completely punting on error handling until we have time to refactor. I'd rather not
// add more ffi error copypasta in the meantime.

/// Instantiate a places connection. Returned connection must be freed with
/// `places_connection_destroy`. Returns null and logs on errors (for now).
#[no_mangle]
pub unsafe extern "C" fn places_connection_new(
    db_path: *const c_char,
    encryption_key: *const c_char,
    error: &mut ExternError,
) -> *mut PlacesDb {
    trace!("places_connection_new");
    logging_init();
    call_with_result(error, || {
        let path = ffi_support::rust_string_from_c(db_path);
        let key = ffi_support::opt_rust_string_from_c(encryption_key);
        PlacesDb::open(path, key.as_ref().map(|v| v.as_str()))
    })
}

/// Add an observation to the database. The observation is a VisitObservation represented as JSON.
/// Errors are logged.
#[no_mangle]
pub unsafe extern "C" fn places_note_observation(
    conn: &mut PlacesDb,
    json_observation: *const c_char,
    error: &mut ExternError,
) {
    trace!("places_note_observation");
    call_with_result(error, || {
        let json = ffi_support::rust_str_from_c(json_observation);
        let visit: places::VisitObservation = serde_json::from_str(&json)?;
        places::api::apply_observation(conn, visit)
    })
}

/// Execute a query, returning a `Vec<SearchResult>` as a JSON string. Returned string must be freed
/// using `places_destroy_string`. Returns null and logs on errors (for now).
#[no_mangle]
pub unsafe extern "C" fn places_query_autocomplete(
    conn: &PlacesDb,
    search: *const c_char,
    limit: u32,
    error: &mut ExternError,
) -> *mut c_char {
    trace!("places_query_autocomplete");
    call_with_result(error, || {
        search_frecent(conn, SearchParams {
            search_string: ffi_support::rust_string_from_c(search),
            limit,
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn places_get_visited(
    conn: &PlacesDb,
    urls_json: *const c_char,
    error: &mut ExternError,
) -> *mut c_char {
    trace!("places_get_visited");
    // This function has a dumb amount of overhead and copying...
    call_with_result(error, || -> places::Result<String> {
        let json = ffi_support::rust_str_from_c(urls_json);
        let url_strings: Vec<String> = serde_json::from_str(json)?;
        let urls = url_strings
            .into_iter()
            .map(|url| url::Url::parse(&url))
            .collect::<Result<Vec<_>, _>>()?;
        // We need to call `to_string` manually because primitives (e.g. bool) don't implement
        // `ffi_support::IntoFfiJsonTag` (Not clear if they should, needs more thought).
        let visited = storage::get_visited(conn, &urls)?;
        Ok(serde_json::to_string(&visited)?)
    })
}


#[no_mangle]
pub extern "C" fn places_get_visited_urls_in_range(
    conn: &PlacesDb,
    start: i64,
    end: i64,
    include_remote: u8, // JNA has issues with bools...
    error: &mut ExternError,
) -> *mut c_char {
    trace!("places_get_visited_in_range");
    call_with_result(error, || -> places::Result<String> {
        let visited = storage::get_visited_urls(
            conn,
            // Probably should allow into()...
            places::Timestamp(start.max(0) as u64),
            places::Timestamp(end.max(0) as u64),
            include_remote != 0
        )?;
        Ok(serde_json::to_string(&visited)?)
    })
}


define_string_destructor!(places_destroy_string);
define_box_destructor!(PlacesDb, places_connection_destroy);
