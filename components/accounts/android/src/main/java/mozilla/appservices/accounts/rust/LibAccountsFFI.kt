@file:Suppress("MaxLineLength")
/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.accounts.rust

import com.sun.jna.Library
import com.sun.jna.Pointer
import mozilla.appservices.support.native.RustBuffer
import mozilla.appservices.support.native.loadIndirect
import org.mozilla.appservices.accounts.BuildConfig

@Suppress("FunctionNaming", "FunctionParameterNaming", "LongParameterList", "TooGenericExceptionThrown")
internal interface LibAccountsFFI : Library {
    companion object {
        internal var INSTANCE: LibAccountsFFI =
            loadIndirect(componentName = "accounts", componentVersion = BuildConfig.LIBRARY_VERSION)
    }

    // TODO: we should just protobuf that thing.
    fun fxa_mgr_new(
        contentUrl: String,
        clientId: String,
        redirectUri: String,
        deviceName: String,
        deviceType: Int,
        capabilities_data: Pointer,
        capabilities_len: Int,
        e: RustError.ByReference
    ): ManagerHandle

    fun fxa_mgr_init(
        mgr: ManagerHandle,
        jsonState: String?,
        e: RustError.ByReference
    )

    fun fxa_mgr_begin_oauth_flow(
        mgr: ManagerHandle,
        e: RustError.ByReference
    ): Pointer?

    fun fxa_mgr_begin_pairing_flow(
        mgr: ManagerHandle,
        pairingUrl: String,
        e: RustError.ByReference
    ): Pointer?

    fun fxa_mgr_finish_authentication_flow(
        mgr: ManagerHandle,
        code: String,
        state: String,
        e: RustError.ByReference
    )

    fun fxa_mgr_on_authentication_error(
        mgr: ManagerHandle,
        e: RustError.ByReference
    )

    fun fxa_mgr_get_profile(
        mgr: ManagerHandle,
        e: RustError.ByReference
    ): RustBuffer.ByValue

    fun fxa_mgr_update_profile(
        mgr: ManagerHandle,
        e: RustError.ByReference
    ): RustBuffer.ByValue

    fun fxa_mgr_logout(
        mgr: ManagerHandle,
        e: RustError.ByReference
    )

    fun fxa_mgr_account_state(
        mgr: ManagerHandle,
        e: RustError.ByReference
    ): RustBuffer.ByValue

    fun fxa_mgr_export_persisted_state(
        mgr: ManagerHandle,
        e: RustError.ByReference
    ): Pointer?

    fun fxa_mgr_update_devices(
        mgr: ManagerHandle,
        e: RustError.ByReference
    ): RustBuffer.ByValue

    fun fxa_mgr_get_devices(
        mgr: ManagerHandle,
        e: RustError.ByReference
    ): RustBuffer.ByValue

    fun fxa_mgr_handle_push_message(
        mgr: ManagerHandle,
        jsonPayload: String,
        e: RustError.ByReference
    ): RustBuffer.ByValue

    fun fxa_mgr_set_device_name(
        mgr: ManagerHandle,
        displayName: String,
        e: RustError.ByReference
    )

    fun fxa_mgr_poll_device_commands(
        mgr: ManagerHandle,
        e: RustError.ByReference
    ): RustBuffer.ByValue

    fun fxa_mgr_set_push_subscription(
        mgr: ManagerHandle,
        endpoint: String,
        publicKey: String,
        authKey: String,
        e: RustError.ByReference
    )

    fun fxa_mgr_send_tab(
        mgr: ManagerHandle,
        targetDeviceId: String,
        title: String,
        url: String,
        e: RustError.ByReference
    )

    fun fxa_mgr_str_free(string: Pointer)
    fun fxa_mgr_bytebuffer_free(buffer: RustBuffer.ByValue)
    fun fxa_mgr_free(mgr: ManagerHandle, err: RustError.ByReference)
}
internal typealias ManagerHandle = Long
