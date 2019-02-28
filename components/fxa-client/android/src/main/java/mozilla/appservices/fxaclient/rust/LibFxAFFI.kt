/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.fxaclient.rust

import android.util.Log
import com.sun.jna.Library
import com.sun.jna.Native
import com.sun.jna.Pointer
import java.lang.reflect.Proxy
import mozilla.appservices.support.RustBuffer

@Suppress("FunctionNaming", "FunctionParameterNaming", "TooManyFunctions", "TooGenericExceptionThrown")
internal interface LibFxAFFI : Library {
    companion object {
        private val JNA_LIBRARY_NAME = {
            val libname = System.getProperty("mozilla.appservices.fxaclient_ffi_lib_name")
            if (libname != null) {
                Log.i("AppServices", "Using fxaclient_ffi_lib_name: " + libname)
                libname
            } else {
                "fxaclient_ffi"
            }
        }()

        internal var INSTANCE: LibFxAFFI = try {
            val lib = Native.loadLibrary(JNA_LIBRARY_NAME, LibFxAFFI::class.java) as LibFxAFFI
            if (JNA_LIBRARY_NAME == "fxaclient_ffi") {
                // Enable logcat logging if we aren't in a megazord.
                lib.fxa_enable_logcat_logging()
            }
            lib
        } catch (e: UnsatisfiedLinkError) {
            Proxy.newProxyInstance(
                    LibFxAFFI::class.java.classLoader,
                    arrayOf(LibFxAFFI::class.java)) { _, _, _ ->
                throw RuntimeException("Firefox Account functionality not available", e)
            } as LibFxAFFI
        }
    }

    fun fxa_enable_logcat_logging()

    fun fxa_new(
        contentUrl: String,
        clientId: String,
        redirectUri: String,
        e: RustError.ByReference
    ): FxaHandle

    fun fxa_from_json(json: String, e: RustError.ByReference): FxaHandle
    fun fxa_to_json(fxa: Long, e: RustError.ByReference): Pointer?

    fun fxa_begin_oauth_flow(
        fxa: FxaHandle,
        scopes: String,
        wantsKeys: Boolean,
        e: RustError.ByReference
    ): Pointer?

    fun fxa_begin_pairing_flow(
        fxa: FxaHandle,
        pairingUrl: String,
        scopes: String,
        e: RustError.ByReference
    ): Pointer?

    fun fxa_profile(fxa: FxaHandle, ignoreCache: Boolean, e: RustError.ByReference): RustBuffer.ByValue

    fun fxa_get_token_server_endpoint_url(fxa: FxaHandle, e: RustError.ByReference): Pointer?
    fun fxa_get_connection_success_url(fxa: FxaHandle, e: RustError.ByReference): Pointer?

    fun fxa_complete_oauth_flow(fxa: FxaHandle, code: String, state: String, e: RustError.ByReference)
    fun fxa_get_access_token(fxa: FxaHandle, scope: String, e: RustError.ByReference): RustBuffer.ByValue

    fun fxa_migrate_from_session_token(fxa: FxaHandle, sessionToken: String, kSync: String, kXCS: String, e: RustError.ByReference)

    fun fxa_str_free(string: Pointer)
    fun fxa_bytebuffer_free(buffer: RustBuffer.ByValue)
    fun fxa_free(fxa: FxaHandle, err: RustError.ByReference)
}
internal typealias FxaHandle = Long
