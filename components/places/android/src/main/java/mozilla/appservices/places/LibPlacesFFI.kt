@file:Suppress("MaxLineLength")
/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.places

import com.sun.jna.Library
import com.sun.jna.Pointer
import com.sun.jna.PointerType
import mozilla.appservices.support.native.loadIndirect
import org.mozilla.appservices.places.BuildConfig

import mozilla.appservices.support.native.RustBuffer

@Suppress("FunctionNaming", "FunctionParameterNaming", "LongParameterList", "TooGenericExceptionThrown")
internal interface LibPlacesFFI : Library {
    companion object {
        internal var INSTANCE: LibPlacesFFI =
            loadIndirect(componentName = "places", componentVersion = BuildConfig.LIBRARY_VERSION)
    }

    // Important: strings returned from rust as *mut char must be Pointers on this end, returning a
    // String will work but either force us to leak them, or cause us to corrupt the heap (when we
    // free them).

    /** Create a new places api */
    fun places_api_new(
        db_path: String,
        out_err: RustError.ByReference
    ): PlacesApiHandle

    /** Create a new places connection */
    fun places_connection_new(
        handle: PlacesApiHandle,
        conn_type: Int,
        out_err: RustError.ByReference
    ): PlacesConnectionHandle

    // Returns a JSON string containing bookmark import metrics
    fun places_bookmarks_import_from_fennec(
        handle: PlacesApiHandle,
        db_path: String,
        out_err: RustError.ByReference
    ): Pointer?

    fun places_pinned_sites_import_from_fennec(
        handle: PlacesApiHandle,
        db_path: String,
        out_err: RustError.ByReference
    ): RustBuffer.ByValue

    // Returns a JSON string containing import metrics
    fun places_history_import_from_fennec(
        handle: PlacesApiHandle,
        db_path: String,
        out_err: RustError.ByReference
    ): Pointer?

    fun places_note_observation(
        handle: PlacesConnectionHandle,
        json_observation_data: String,
        out_err: RustError.ByReference
    )

    /** Returns a URL, or null if no match was found. */
    fun places_match_url(
        handle: PlacesConnectionHandle,
        search: String,
        out_err: RustError.ByReference
    ): Pointer?

    fun places_new_interrupt_handle(
        conn: PlacesConnectionHandle,
        out_err: RustError.ByReference
    ): RawPlacesInterruptHandle?

    fun places_new_sync_conn_interrupt_handle(
        api: PlacesApiHandle,
        out_err: RustError.ByReference
    ): RawPlacesInterruptHandle?

    fun places_interrupt(
        conn: RawPlacesInterruptHandle,
        out_err: RustError.ByReference
    )

    fun bookmarks_get_all_with_url(
        handle: PlacesConnectionHandle,
        url: String,
        error: RustError.ByReference
    ): RustBuffer.ByValue

    fun bookmarks_get_url_for_keyword(
        handle: PlacesConnectionHandle,
        keyword: String,
        error: RustError.ByReference
    ): Pointer?

    fun bookmarks_get_tree(
        handle: PlacesConnectionHandle,
        optRootId: String?,
        error: RustError.ByReference
    ): RustBuffer.ByValue

    fun bookmarks_get_by_guid(
        handle: PlacesConnectionHandle,
        optRootId: String?,
        getDirectChildren: Byte,
        error: RustError.ByReference
    ): RustBuffer.ByValue

    fun bookmarks_search(
        handle: PlacesConnectionHandle,
        search: String,
        limit: Int,
        error: RustError.ByReference
    ): RustBuffer.ByValue

    fun bookmarks_get_recent(
        handle: PlacesConnectionHandle,
        limit: Int,
        error: RustError.ByReference
    ): RustBuffer.ByValue

    // Returns newly inserted guid
    fun bookmarks_insert(
        handle: PlacesConnectionHandle,
        data: Pointer,
        len: Int,
        error: RustError.ByReference
    ): Pointer?

    fun bookmarks_update(
        handle: PlacesConnectionHandle,
        data: Pointer,
        len: Int,
        error: RustError.ByReference
    )

    // Returns 1 if the item existed and was deleted.
    fun bookmarks_delete(
        handle: PlacesConnectionHandle,
        id: String,
        error: RustError.ByReference
    ): Byte

    fun bookmarks_delete_everything(
        handle: PlacesConnectionHandle,
        error: RustError.ByReference
    )

    fun bookmarks_reset(
        handle: PlacesApiHandle,
        error: RustError.ByReference
    )

    /** Destroy strings returned from libplaces_ffi calls. */
    fun places_destroy_string(s: Pointer)

    fun places_api_return_write_conn(
        apiHandle: PlacesApiHandle,
        writeHandle: PlacesConnectionHandle,
        err: RustError.ByReference
    )

    /** Destroy connection created using `places_connection_new` */
    fun places_connection_destroy(handle: PlacesConnectionHandle, out_err: RustError.ByReference)

    /** Destroy api created using `places_api_new` */
    fun places_api_destroy(handle: PlacesApiHandle, out_err: RustError.ByReference)

    /** Destroy handle created using `places_new_interrupt_handle` */
    fun places_interrupt_handle_destroy(obj: RawPlacesInterruptHandle)

    fun places_destroy_bytebuffer(bb: RustBuffer.ByValue)
}

internal typealias PlacesConnectionHandle = Long
internal typealias PlacesApiHandle = Long

// This doesn't use a handle to avoid unnecessary locking and
// because the type is panic safe, sync, and send.
class RawPlacesInterruptHandle : PointerType()
