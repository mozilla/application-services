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

    fun places_note_observation(
        handle: PlacesConnectionHandle,
        json_observation_data: String,
        out_err: RustError.ByReference
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
