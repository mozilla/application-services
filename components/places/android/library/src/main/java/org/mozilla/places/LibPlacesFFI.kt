/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.places

import com.sun.jna.Library
import com.sun.jna.Native
import com.sun.jna.Pointer
import com.sun.jna.PointerType

internal interface LibPlacesFFI : Library {
    companion object {
        private const val JNA_LIBRARY_NAME = "places_ffi"
        internal var INSTANCE: LibPlacesFFI

        init {
            INSTANCE = Native.loadLibrary(JNA_LIBRARY_NAME, LibPlacesFFI::class.java) as LibPlacesFFI
        }
    }

    // Important: strings returned from rust as *mut char must be Pointers on this end, returning a
    // String will work but either force us to leak them, or cause us to corrupt the heap (when we
    // free them).

    /** Create a new places connection */
    fun places_connection_new(
            db_path: String,
            encryption_key: String?,
            out_err: RustError.ByReference
    ): RawPlacesConnection?

    /** Returns JSON string, which you need to free with places_destroy_string */
    fun places_note_observation(
            conn: RawPlacesConnection,
            json_observation_data: String,
            out_err: RustError.ByReference
    )

    /** Returns JSON string, which you need to free with places_destroy_string */
    fun places_query_autocomplete(
            conn: RawPlacesConnection,
            search: String,
            limit: Int,
            out_err: RustError.ByReference
    ): Pointer?

    /** Destroy strings returned from libplaces_ffi calls. */
    fun places_destroy_string(s: Pointer)

    /** Destroy connection created using `places_connection_new` */
    fun places_connection_destroy(obj: RawPlacesConnection)
}

class RawPlacesConnection : PointerType()
