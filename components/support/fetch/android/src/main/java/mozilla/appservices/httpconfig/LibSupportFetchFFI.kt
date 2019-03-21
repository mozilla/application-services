/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.httpconfig

import android.util.Log
import com.sun.jna.Library
import com.sun.jna.Callback
import com.sun.jna.Native
import java.lang.reflect.Proxy
import mozilla.appservices.support.RustBuffer

internal interface LibSupportFetchFFI : Library {
    companion object {
        private val JNA_LIBRARY_NAME = {
            val libname = System.getProperty("mozilla.appservices.support_fetch_lib_name")
            if (libname != null) {
                Log.i("AppServices", "Using support_fetch_lib_name: " + libname);
                libname
            } else {
                "support_fetch"
            }
        }()

        internal var INSTANCE: LibSupportFetchFFI = try {
            val lib = Native.loadLibrary(JNA_LIBRARY_NAME, LibSupportFetchFFI::class.java) as LibSupportFetchFFI
            if (JNA_LIBRARY_NAME == "support_fetch") {
                // TODO Enable logging if we aren't in a megazord.
            }
            lib
        } catch (e: UnsatisfiedLinkError) {
            Proxy.newProxyInstance(
                    LibSupportFetchFFI::class.java.classLoader,
                    arrayOf(LibSupportFetchFFI::class.java))
            { _, _, _ ->
                throw RuntimeException("LibSupportFetchFFI not available", e)
            } as LibSupportFetchFFI
        }
    }

    fun support_fetch_destroy_bytebuffer(b: RustBuffer.ByValue)
    // Returns null buffer to indicate failure
    fun support_fetch_alloc_bytebuffer(sz: Int): RustBuffer.ByValue
    // Returns 0 to indicate redundant init.
    fun support_fetch_initialize(cb: RawFetchCallback): Byte
}

internal interface RawFetchCallback : Callback {
    fun invoke(b: RustBuffer.ByValue): RustBuffer.ByValue
}


