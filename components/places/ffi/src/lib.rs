/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use ffi_support::{
    define_box_destructor, define_bytebuffer_destructor, define_handle_map_deleter,
    define_string_destructor, ByteBuffer, ConcurrentHandleMap, ExternError, FfiStr,
};
use places::error::*;
use places::msg_types::BookmarkNodeList;
use places::storage::bookmarks;
use places::types::SyncGuid;
use places::{db::PlacesInterruptHandle, storage, ConnectionType, PlacesApi, PlacesDb};
use std::os::raw::c_char;
use std::sync::Arc;

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
    static ref APIS: ConcurrentHandleMap<Arc<PlacesApi>> = ConcurrentHandleMap::new();
    static ref CONNECTIONS: ConcurrentHandleMap<PlacesDb> = ConcurrentHandleMap::new();
}

/// Instantiate a places API. Returned api must be freed with
/// `places_api_destroy`. Returns null and logs on errors (for now).
#[no_mangle]
pub extern "C" fn places_api_new(
    db_path: FfiStr<'_>,
    encryption_key: FfiStr<'_>,
    error: &mut ExternError,
) -> u64 {
    log::debug!("places_api_new");
    APIS.insert_with_result(error, || {
        let path = db_path.as_str();
        let key = encryption_key.as_opt_str();
        PlacesApi::new(path, key)
    })
}

/// Instantiate a places connection. Returned connection must be freed with
/// `places_connection_destroy`, although it should be noted that this will
/// not destroy, or even close, connections already obtained from this
/// object. Returns null and logs on errors (for now).
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
pub extern "C" fn places_note_observation(
    handle: u64,
    json_observation: FfiStr<'_>,
    error: &mut ExternError,
) {
    log::debug!("places_note_observation");
    CONNECTIONS.call_with_result_mut(error, handle, |conn| {
        let json = json_observation.as_str();
        let visit: places::VisitObservation = serde_json::from_str(&json)?;
        places::api::apply_observation(conn, visit)
    })
}

/// Execute a query, returning a `Vec<SearchResult>` as a JSON string. Returned string must be freed
/// using `places_destroy_string`. Returns null and logs on errors (for now).
#[no_mangle]
pub extern "C" fn places_query_autocomplete(
    handle: u64,
    search: FfiStr<'_>,
    limit: u32,
    error: &mut ExternError,
) -> *mut c_char {
    log::debug!("places_query_autocomplete");
    CONNECTIONS.call_with_result(error, handle, |conn| -> places::Result<_> {
        let res = search_frecent(
            conn,
            SearchParams {
                search_string: search.into_string(),
                limit,
            },
        )?;
        Ok(serde_json::to_string(&res)?)
    })
}

/// Execute a query, returning a URL string or null. Returned string must be freed
/// using `places_destroy_string`. Returns null if no match is found.
#[no_mangle]
pub extern "C" fn places_match_url(
    handle: u64,
    search: FfiStr<'_>,
    error: &mut ExternError,
) -> *mut c_char {
    log::debug!("places_match_url");
    CONNECTIONS.call_with_result(error, handle, |conn| match_url(conn, search.as_str()))
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
                let s = FfiStr::from_raw(p).as_str();
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
pub extern "C" fn places_delete_place(handle: u64, url: FfiStr<'_>, error: &mut ExternError) {
    log::debug!("places_delete_place");
    CONNECTIONS.call_with_result(error, handle, |conn| -> places::Result<_> {
        let url = parse_url(url.as_str())?;
        if let Some(guid) = storage::history::url_to_guid(conn, &url)? {
            storage::history::delete_place_by_guid(conn, &guid)?;
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn places_delete_visits_between(
    handle: u64,
    start: i64,
    end: i64,
    error: &mut ExternError,
) {
    log::debug!("places_delete_visits_between");
    CONNECTIONS.call_with_result(error, handle, |conn| -> places::Result<_> {
        storage::history::delete_visits_between(
            conn,
            places::Timestamp(start.max(0) as u64),
            places::Timestamp(end.max(0) as u64),
        )?;
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn places_delete_visit(
    handle: u64,
    url: FfiStr<'_>,
    timestamp: i64,
    error: &mut ExternError,
) {
    log::debug!("places_delete_visit");
    CONNECTIONS.call_with_result(error, handle, |conn| -> places::Result<_> {
        storage::history::delete_place_visit_at_time(
            conn,
            &parse_url(url.as_str())?,
            places::Timestamp(timestamp.max(0) as u64),
        )?;
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn places_wipe_local(handle: u64, error: &mut ExternError) {
    log::debug!("places_wipe_local");
    CONNECTIONS.call_with_result(error, handle, |conn| storage::history::wipe_local(conn))
}

#[no_mangle]
pub extern "C" fn places_run_maintenance(handle: u64, error: &mut ExternError) {
    log::debug!("places_run_maintenance");
    CONNECTIONS.call_with_result(error, handle, |conn| storage::run_maintenance(conn))
}

#[no_mangle]
pub extern "C" fn places_prune_destructively(handle: u64, error: &mut ExternError) {
    log::debug!("places_prune_destructively");
    CONNECTIONS.call_with_result(error, handle, |conn| {
        storage::history::prune_destructively(conn)
    })
}

#[no_mangle]
pub extern "C" fn places_delete_everything(handle: u64, error: &mut ExternError) {
    log::debug!("places_delete_everything");
    CONNECTIONS.call_with_result(error, handle, |conn| {
        storage::history::delete_everything(conn)
    })
}

#[no_mangle]
pub extern "C" fn places_get_visit_infos(
    handle: u64,
    start_date: i64,
    end_date: i64,
    error: &mut ExternError,
) -> ByteBuffer {
    log::debug!("places_get_visit_infos");
    CONNECTIONS.call_with_result(error, handle, |conn| -> places::Result<_> {
        Ok(storage::history::get_visit_infos(
            conn,
            places::Timestamp(start_date.max(0) as u64),
            places::Timestamp(end_date.max(0) as u64),
        )?)
    })
}

#[no_mangle]
pub extern "C" fn sync15_history_sync(
    handle: u64,
    key_id: FfiStr<'_>,
    access_token: FfiStr<'_>,
    sync_key: FfiStr<'_>,
    tokenserver_url: FfiStr<'_>,
    error: &mut ExternError,
) {
    log::debug!("sync15_history_sync");
    APIS.call_with_result(error, handle, |api| -> places::Result<_> {
        // Note that api.sync returns a SyncPing which we drop on the floor.
        api.sync(
            &sync15::Sync15StorageClientInit {
                key_id: key_id.into_string(),
                access_token: access_token.into_string(),
                tokenserver_url: parse_url(tokenserver_url.as_str())?,
            },
            &sync15::KeyBundle::from_ksync_base64(sync_key.as_str())?,
        )?;
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn bookmarks_get_tree(
    handle: u64,
    guid: FfiStr<'_>,
    error: &mut ExternError,
) -> ByteBuffer {
    log::debug!("bookmarks_get_tree");
    CONNECTIONS.call_with_result(error, handle, |conn| -> places::Result<_> {
        let root_id = SyncGuid(guid.into());
        Ok(bookmarks::public_node::fetch_public_tree(conn, &root_id)?)
    })
}

#[no_mangle]
pub extern "C" fn bookmarks_get_by_guid(
    handle: u64,
    guid: FfiStr<'_>,
    get_direct_children: u8,
    error: &mut ExternError,
) -> ByteBuffer {
    log::debug!("bookmarks_get_by_guid");
    CONNECTIONS.call_with_result(error, handle, |conn| -> places::Result<_> {
        let guid = SyncGuid(guid.into());
        Ok(bookmarks::public_node::fetch_bookmark(
            conn,
            &guid,
            get_direct_children != 0,
        )?)
    })
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
pub unsafe extern "C" fn bookmarks_insert(
    handle: u64,
    data: *const u8,
    len: i32,
    error: &mut ExternError,
) -> *mut c_char {
    log::debug!("bookmarks_insert");
    use places::msg_types::BookmarkNode;
    CONNECTIONS.call_with_result(error, handle, |conn| -> places::Result<_> {
        let buffer = get_buffer(data, len);
        let bookmark: BookmarkNode = prost::Message::decode(buffer)?;
        let insertable = bookmark.into_insertable()?;
        let guid = bookmarks::insert_bookmark(conn, &insertable)?;
        Ok(guid.0)
    })
}

#[no_mangle]
pub unsafe extern "C" fn bookmarks_update(
    handle: u64,
    data: *const u8,
    len: i32,
    error: &mut ExternError,
) {
    log::debug!("bookmarks_update");
    use places::msg_types::BookmarkNode;
    CONNECTIONS.call_with_result(error, handle, |conn| -> places::Result<_> {
        let buffer = get_buffer(data, len);
        let bookmark: BookmarkNode = prost::Message::decode(buffer)?;
        bookmarks::public_node::update_bookmark_from_message(conn, bookmark)?;
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn bookmarks_delete(handle: u64, id: FfiStr<'_>, error: &mut ExternError) -> u8 {
    log::debug!("bookmarks_delete");
    CONNECTIONS.call_with_result(error, handle, |conn| -> places::Result<_> {
        let guid = SyncGuid(id.into_string());
        let did_delete = bookmarks::delete_bookmark(conn, &guid)?;
        Ok(did_delete)
    })
}

#[no_mangle]
pub extern "C" fn bookmarks_get_all_with_url(
    handle: u64,
    url: FfiStr<'_>,
    error: &mut ExternError,
) -> ByteBuffer {
    log::debug!("bookmarks_get_all_with_url");
    CONNECTIONS.call_with_result(error, handle, |conn| -> places::Result<_> {
        Ok(BookmarkNodeList::from(
            bookmarks::public_node::fetch_bookmarks_by_url(conn, &parse_url(url.as_str())?)?,
        ))
    })
}

#[no_mangle]
pub extern "C" fn bookmarks_search(
    handle: u64,
    query: FfiStr<'_>,
    limit: i32,
    error: &mut ExternError,
) -> ByteBuffer {
    log::debug!("bookmarks_get_all_with_url");
    CONNECTIONS.call_with_result(error, handle, |conn| -> places::Result<_> {
        Ok(BookmarkNodeList::from(
            bookmarks::public_node::search_bookmarks(conn, query.as_str(), limit as u32)?,
        ))
    })
}

define_string_destructor!(places_destroy_string);
define_bytebuffer_destructor!(places_destroy_bytebuffer);
define_handle_map_deleter!(APIS, places_api_destroy);

define_handle_map_deleter!(CONNECTIONS, places_connection_destroy);
define_box_destructor!(PlacesInterruptHandle, places_interrupt_handle_destroy);
