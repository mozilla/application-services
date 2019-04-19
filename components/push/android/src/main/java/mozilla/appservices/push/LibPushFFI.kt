@file:Suppress("MaxLineLength")
/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.push

import android.util.Log
import com.sun.jna.Library
import com.sun.jna.Native
import com.sun.jna.Pointer
import java.lang.reflect.Proxy

import mozilla.appservices.support.RustBuffer

@Suppress("FunctionNaming", "FunctionParameterNaming", "LongParameterList", "TooGenericExceptionThrown")
internal interface LibPushFFI : Library {
    companion object {
        private val JNA_LIBRARY_NAME = {
            val libname = System.getProperty("mozilla.appservices.push_ffi_lib_name")
            if (libname != null) {
                Log.i("AppServices", "Using push_ffi_lib_name: {$libname}")
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
                arrayOf(LibPushFFI::class.java)) { _, _, _ ->
                throw RuntimeException("Push functionality not available", e)
            } as LibPushFFI
        }
    }

    // Important: strings returned from rust as *mut char must be Pointers on this end, returning a
    // String will work but either force us to leak them, or cause us to corrupt the heap (when we
    // free them).

    /** Create a new push connection */
    fun push_connection_new(
        server_host: String,
        http_protocol: String?,
        bridge_type: String?,
        registration_id: String,
        sender_id: String?,
        database_path: String,
        out_err: RustError.ByReference
    ): PushManagerHandle

    /** Returns JSON string, which you need to free with push_destroy_string */
    fun push_subscribe(
        mgr: PushManagerHandle,
        channel_id: String,
        scope: String,
        out_err: RustError.ByReference
    ): Pointer?

    /** Returns bool */
    fun push_unsubscribe(
        mgr: PushManagerHandle,
        channel_id: String,
        out_err: RustError.ByReference
    ): Byte

    fun push_update(
        mgr: PushManagerHandle,
        new_token: String,
        out_err: RustError.ByReference
    ): Byte

    fun push_verify_connection(
        mgr: PushManagerHandle,
        out_err: RustError.ByReference
    ): Pointer?

    fun push_decrypt(
        mgr: PushManagerHandle,
        channel_id: String,
        body: String,
        encoding: String,
        salt: String?,
        dh: String?,
        out_err: RustError.ByReference
    ): Pointer?

    fun push_dispatch_for_chid(
        mgr: PushManagerHandle,
        channelID: String,
        out_err: RustError.ByReference
    ): Pointer?

    /** Destroy strings returned from libpush_ffi calls. */
    fun push_destroy_string(s: Pointer)

    /** Destroy a buffer value returned from the decrypt ffi call */
    fun push_destroy_buffer(s: RustBuffer.ByValue)

    /** Destroy connection created using `push_connection_new` */
    fun push_connection_destroy(obj: PushManagerHandle, out_err: RustError.ByReference)
}

internal typealias PushManagerHandle = Long
