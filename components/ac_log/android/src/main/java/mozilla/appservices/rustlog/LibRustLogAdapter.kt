/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.rustlog

import android.util.Log
import com.sun.jna.Callback
import com.sun.jna.Library
import com.sun.jna.Native
import com.sun.jna.Pointer
import com.sun.jna.PointerType
import java.lang.reflect.Proxy

@Suppress("FunctionNaming", "TooManyFunctions", "TooGenericExceptionThrown")
internal interface LibRustLogAdapter : Library {
    companion object {
        private val JNA_LIBRARY_NAME = {
            val libname = System.getProperty("mozilla.appservices.ac_rust_log_lib_name")
            if (libname != null) {
                Log.i("AppServices", "Using ac_rust_log_lib_name: " + libname);
                libname
            } else {
                "ac_rust_log"
            }
        }()

        internal var INSTANCE: LibRustLogAdapter = try {
            Native.loadLibrary(JNA_LIBRARY_NAME, LibRustLogAdapter::class.java) as LibRustLogAdapter
        } catch (e: UnsatisfiedLinkError) {
            Proxy.newProxyInstance(
                    LibRustLogAdapter::class.java.classLoader,
                    arrayOf(LibRustLogAdapter::class.java))
            { _, _, _ ->
                throw RuntimeException("Rust log functionality not available (no native library)", e)
            } as LibRustLogAdapter
        }
    }

    fun ac_log_adapter_create(
        callback: RawLogCallback,
        out_err: RustError.ByReference
    ): RawLogAdapter?

    fun ac_log_adapter_set_max_level(
        adapter: RawLogAdapter,
        level: Int,
        out_err: RustError.ByReference
    )

    fun ac_log_adapter_destroy(a: RawLogAdapter)
    fun ac_log_adapter_destroy_string(p: Pointer)

    // Only call this from tests!
    fun ac_log_adapter_test__log_msg(s: String)
}

interface RawLogCallback : Callback {
    fun invoke(level: Int, tag: Pointer?, message: Pointer): Byte;
}

class RawLogAdapter : PointerType()
