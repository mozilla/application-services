/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.push

import android.util.Log
import com.sun.jna.Library
import com.sun.jna.Native
import com.sun.jna.Pointer
import com.sun.jna.PointerType
import java.lang.reflect.Proxy

internal interface LibPushFFI : Library {
    companion object {
        private val JNA_LIBRARY_NAME = {
            val libname = System.getProperty("mozilla.appservices.push_ffi_lib_name")
            if (libname != null) {
                Log.i("AppServices", "Using push_ffi_lib_name: " + libname);
                libname
            } else {
                "push_ffi"
            }
        }()

        internal var INSTANCE: LibPushFFI = try {
            Native.loadLibrary(JNA_LIBRARY_NAME, LibPushFFI::class.java) as LibPushFFI
        } catch (e: UnsatisfiedLinkError) {
            Proxy.newProxyInstance(
                LibPushFFI::class.java.classLoader,
                arrayOf(LibPushFFI::class.java))
            { _, _, _ ->
                throw RuntimeException("Push functionality not available", e)
            } as LibPushFFI
        }
    }

    // Important: strings returned from rust as *mut char must be Pointers on this end, returning a
    // String will work but either force us to leak them, or cause us to corrupt the heap (when we
    // free them).

    /* From places
    / ** Create a new push connection */
    fun push_connection_new(
            db_path: String,
            encryption_key: String?,
            out_err: RustError.ByReference
    ): RawPushConnection?

    / ** Returns JSON string, which you need to free with push_destroy_string */
    fun push_note_observation(
            conn: RawPushConnection,
            json_observation_data: String,
            out_err: RustError.ByReference
    )

    / ** Returns JSON string, which you need to free with push_destroy_string */
    fun push_query_autocomplete(
            conn: RawPushConnection,
            search: String,
            limit: Int,
            out_err: RustError.ByReference
    ): Pointer?

    fun push_get_visited(
            conn: RawPushConnection,
            urls_json: String,
            out_err: RustError.ByReference
    ): Pointer?

    fun push_get_visited_urls_in_range(
            conn: RawPushConnection,
            start: Long,
            end: Long,
            include_remote: Byte,
            out_err: RustError.ByReference
    ): Pointer?

    fun sync15_history_sync(
            conn: RawPushConnection,
            key_id: String,
            access_token: String,
            sync_key: String,
            tokenserver_url: String,
            out_err: RustError.ByReference
    )

    / ** Destroy strings returned from libpush_ffi calls. */
    fun push_destroy_string(s: Pointer)

    / ** Destroy connection created using `push_connection_new` */
    fun push_connection_destroy(obj: RawPushConnection)

    */
}

class RawPushConnection : PointerType()
