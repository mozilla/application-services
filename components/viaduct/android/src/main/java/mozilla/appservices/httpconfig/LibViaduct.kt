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

@Suppress("FunctionNaming", "TooGenericExceptionThrown")
internal interface LibViaduct : Library {
    companion object {
        private val JNA_LIBRARY_NAME = {
            val libname = System.getProperty("mozilla.appservices.viaduct_lib_name")
            if (libname != null) {
                Log.i("AppServices", "Using viaduct_lib_name: " + libname)
                libname
            } else {
                "viaduct"
            }
        }()

        internal var INSTANCE: LibViaduct = try {
            val lib = Native.loadLibrary(JNA_LIBRARY_NAME, LibViaduct::class.java) as LibViaduct
            if (JNA_LIBRARY_NAME == "viaduct") {
                // TODO Enable logging if we aren't in a megazord.
            } else {
                // We're in a megazord. At the moment, we can't prevent cargo
                // from compiling in reqwest. So we force-enable this backend
                // as a stopgap.
                lib.viaduct_force_enable_ffi_backend(1)
            }
            lib
        } catch (e: UnsatisfiedLinkError) {
            Proxy.newProxyInstance(
                    LibViaduct::class.java.classLoader,
                    arrayOf(LibViaduct::class.java)) { _, _, _ ->
                throw RuntimeException("LibViaduct not available", e)
            } as LibViaduct
        }
    }

    fun viaduct_destroy_bytebuffer(b: RustBuffer.ByValue)
    // Returns null buffer to indicate failure
    fun viaduct_alloc_bytebuffer(sz: Int): RustBuffer.ByValue
    // Returns 0 to indicate redundant init.
    fun viaduct_initialize(cb: RawFetchCallback): Byte

    fun viaduct_force_enable_ffi_backend(b: Byte)
}

internal interface RawFetchCallback : Callback {
    fun invoke(b: RustBuffer.ByValue): RustBuffer.ByValue
}
