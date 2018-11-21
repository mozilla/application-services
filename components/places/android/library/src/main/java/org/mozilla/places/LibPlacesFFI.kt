/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.places

import android.util.Log
import com.sun.jna.Library
import com.sun.jna.Native
import com.sun.jna.Pointer
import com.sun.jna.PointerType
import java.lang.reflect.Proxy

internal interface LibPlacesFFI : Library {
    companion object {
        private val JNA_LIBRARY_NAME = {
            val libname = System.getProperty("mozilla.appservices.places_ffi_lib_name")
            if (libname != null) {
                Log.i("AppServices", "Using places_ffi_lib_name: " + libname);
                libname
            } else {
                "places_ffi"
            }
        }()

        internal var INSTANCE: LibPlacesFFI = try {
            Native.loadLibrary(JNA_LIBRARY_NAME, LibPlacesFFI::class.java) as LibPlacesFFI
        } catch (e: UnsatisfiedLinkError) {
            Proxy.newProxyInstance(
                LibPlacesFFI::class.java.classLoader,
                arrayOf(LibPlacesFFI::class.java))
            { _, _, _ ->
                throw RuntimeException("Places functionality not available", e)
            } as LibPlacesFFI
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

    fun places_get_visited(
            conn: RawPlacesConnection,
            urls_json: String,
            out_err: RustError.ByReference
    ): Pointer?

    fun places_get_visited_urls_in_range(
            conn: RawPlacesConnection,
            start: Long,
            end: Long,
            include_remote: Byte,
            out_err: RustError.ByReference
    ): Pointer?

    /** Destroy strings returned from libplaces_ffi calls. */
    fun places_destroy_string(s: Pointer)

    /** Destroy connection created using `places_connection_new` */
    fun places_connection_destroy(obj: RawPlacesConnection)
}

class RawPlacesConnection : PointerType()
