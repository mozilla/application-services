/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.fxaclient.internal

import android.util.Log
import com.sun.jna.Library
import com.sun.jna.Native
import com.sun.jna.Pointer
import com.sun.jna.PointerType
import java.lang.reflect.Proxy

@Suppress("FunctionNaming", "TooManyFunctions", "TooGenericExceptionThrown")
internal interface FxaClient : Library {
    companion object {
        private val JNA_LIBRARY_NAME = {
            val libname = System.getProperty("mozilla.appservices.fxaclient_ffi_lib_name")
            if (libname != null) {
                Log.i("AppServices", "Using fxaclient_ffi_lib_name: " + libname);
                libname
            } else {
                "fxaclient_ffi"
            }
        }()

        internal var INSTANCE: FxaClient

        init {
            try {
                INSTANCE = Native.loadLibrary(JNA_LIBRARY_NAME, FxaClient::class.java) as FxaClient
            } catch (e: UnsatisfiedLinkError) {
                // We want to be able load this class in environments that don't have FxA native
                // libs available (for unit testing purposes). This also has the advantage of
                // not stopping the whole world in case of missing native FxA libs.
                INSTANCE = Proxy.newProxyInstance(
                        FxaClient::class.java.classLoader,
                        arrayOf(FxaClient::class.java)) { _, _, _ ->
                    throw RuntimeException("Firefox Account functionality not available", e)
                } as FxaClient
            }
        }
    }

    fun fxa_new(
        contentUrl: String,
        clientId: String,
        redirectUri: String,
        e: Error.ByReference
    ): RawFxAccount?

    fun fxa_from_json(json: String, e: Error.ByReference): RawFxAccount?
    fun fxa_to_json(fxa: RawFxAccount, e: Error.ByReference): Pointer?

    fun fxa_begin_oauth_flow(
        fxa: RawFxAccount,
        scopes: String,
        wantsKeys: Boolean,
        e: Error.ByReference
    ): Pointer?

    fun fxa_begin_pairing_flow(
        fxa: RawFxAccount,
        pairingUrl: String,
        scopes: String,
        e: Error.ByReference
    ): Pointer?

    fun fxa_profile(fxa: RawFxAccount, ignoreCache: Boolean, e: Error.ByReference): Profile.Raw?

    fun fxa_get_token_server_endpoint_url(fxa: RawFxAccount, e: Error.ByReference): Pointer?
    fun fxa_get_connection_success_url(fxa: RawFxAccount, e: Error.ByReference): Pointer?

    fun fxa_complete_oauth_flow(fxa: RawFxAccount, code: String, state: String, e: Error.ByReference)
    fun fxa_get_access_token(fxa: RawFxAccount, scope: String, e: Error.ByReference): AccessTokenInfo.Raw?

    fun fxa_str_free(string: Pointer)
    fun fxa_free(fxa: RawFxAccount)

    // In theory these would take `AccessTokenInfo.Raw.ByReference` (and etc), but
    // the rust functions that return these return `AccessTokenInfo.Raw` and not
    // the ByReference subtypes. So I'm not sure there's a way to do this
    // when using Structure.
    fun fxa_oauth_info_free(ptr: Pointer)

    fun fxa_profile_free(ptr: Pointer)
}

class RawFxAccount : PointerType()
