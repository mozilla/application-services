/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.accounts

import android.content.Context
import android.util.Log
import androidx.lifecycle.LifecycleOwner
import com.sun.jna.Native
import com.sun.jna.Pointer
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.async
import kotlinx.coroutines.launch
import mozilla.appservices.fxaclient.MsgTypes as FxAMsgTypes
import mozilla.appservices.accounts.rust.LibAccountsFFI
import mozilla.appservices.accounts.rust.RustError
import mozilla.appservices.fxaclient.Profile
import mozilla.appservices.fxaclient.Device
import mozilla.appservices.fxaclient.FxaException
import mozilla.appservices.fxaclient.AccountEvent
import mozilla.appservices.fxaclient.exhaustive
import mozilla.appservices.support.native.toNioDirectBuffer
import mozilla.components.support.base.observer.Observable
import mozilla.components.support.base.observer.ObserverRegistry
import java.nio.ByteBuffer
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicLong
import kotlin.coroutines.CoroutineContext

/**
 * Observer interface which lets its users monitor account state changes and major events.
 */
interface AccountObserver {
    /**
     * Account just got logged out.
     */
    fun onLoggedOut() = Unit

    /**
     * Account was successfully authenticated.
     */
    fun onAuthenticated() = Unit

    /**
     * Account's profile is now available.
     * @param profile A fresh version of account's [Profile].
     */
    fun onProfileUpdated(profile: Profile) = Unit

    /**
     * Account needs to be re-authenticated (e.g. due to a password change).
     */
    fun onAuthenticationProblems() = Unit
}

data class ConstellationState(val currentDevice: Device?, val otherDevices: List<Device>) {
    companion object {
        internal fun fromMessage(msg: FxAMsgTypes.DeviceConstellation): ConstellationState {
            return ConstellationState(
                    currentDevice = Device.fromMessage(msg.currentDevice),
                    otherDevices = Device.fromCollectionMessage(msg.otherDevices).toList()
            )
        }
    }
}

interface DeviceEventsObserver {
    fun onEvents(events: List<AccountEvent>)
}

sealed class DeviceEventOutgoing {
    class SendTab(val title: String, val url: String) : DeviceEventOutgoing()
}

/**
 * Allows monitoring constellation state.
 */
interface DeviceConstellationObserver {
    fun onDevicesUpdate(constellation: ConstellationState)
}

enum class AccountState {
    Start,
    NotAuthenticated,
    Authenticated,
    AuthenticationProblem;

    companion object {
        internal fun fromMessage(msg: MsgTypes.AccountState): AccountState {
            return when (msg.state) {
                MsgTypes.AccountState.State.START -> Start
                MsgTypes.AccountState.State.NOT_AUTHENTICATED -> NotAuthenticated
                MsgTypes.AccountState.State.AUTHENTICATED -> Authenticated
                MsgTypes.AccountState.State.AUTHENTICATION_PROBLEM -> AuthenticationProblem
                null -> throw NullPointerException("AccountState type cannot be null.")
            }.exhaustive
        }
    }
}

/**
 * FirefoxAccount represents the authentication state of a client.
 */
open class FxAccountManager(
    private val context: Context,
    private val serverConfig: ServerConfig,
    private val deviceConfig: DeviceConfig
) : AutoCloseable, Observable<AccountObserver> by ObserverRegistry() {
    private val handle: AtomicLong
    // We want a single-threaded execution model for our account-related "actions" (state machine side-effects).
    // That is, we want to ensure a sequential execution flow, but on a background thread.
    private val coroutineContext: CoroutineContext = Executors
            .newSingleThreadExecutor().asCoroutineDispatcher() + SupervisorJob()
    private val deviceConstellation: FxADeviceConstellation = FxADeviceConstellation()

    init {
        val (nioBuf, len) = capabilitiesToBuffer(deviceConfig.capabilities)
        this.handle = AtomicLong(rustCall { e ->
            val ptr = Native.getDirectBufferPointer(nioBuf)
            LibAccountsFFI.INSTANCE.fxa_mgr_new(
                    serverConfig.contentUrl,
                    serverConfig.clientId,
                    serverConfig.redirectUri,
                    deviceConfig.name,
                    deviceConfig.type.toNumber(),
                    ptr,
                    len,
                    e
            )
        })
        register(observer = object : AccountObserver {
            override fun onAuthenticated() {
                CoroutineScope(coroutineContext).launch {
                    this@FxAccountManager.updateProfileAsync()
                }
            }
        })
    }

    // TODO: shouldn't we just let a-c wrap these methods in async coroutine scopes?
    fun initAsync(): Deferred<Unit> = CoroutineScope(coroutineContext).async {
        val jsonState = getAccountStorage().read()
        rustCallWithLock { e ->
            LibAccountsFFI.INSTANCE.fxa_mgr_init(this@FxAccountManager.handle.get(), jsonState, e)
        }
        persistAccount()
        when (accountState()) {
            AccountState.Authenticated -> notifyObservers { onAuthenticated() } // Account restored.
            AccountState.AuthenticationProblem -> notifyObservers { onAuthenticationProblems() } // Still in a bad state heh.
            else -> { /* Do nothing */ }
        }
    }

    fun beginOAuthFlowAsync(): Deferred<String> = CoroutineScope(coroutineContext).async {
        rustCallWithLock { e ->
            LibAccountsFFI.INSTANCE.fxa_mgr_begin_oauth_flow(this@FxAccountManager.handle.get(), e)
        }.getAndConsumeRustString()
    }

    fun beginPairingFlowAsync(pairingUrl: String): Deferred<String> = CoroutineScope(coroutineContext).async {
        rustCallWithLock { e ->
            LibAccountsFFI.INSTANCE.fxa_mgr_begin_pairing_flow(this@FxAccountManager.handle.get(), pairingUrl, e)
        }.getAndConsumeRustString()
    }

    fun finishAuthenticationAsync(code: String, state: String): Deferred<Unit> = CoroutineScope(coroutineContext).async {
        rustCallWithLock { e ->
            LibAccountsFFI.INSTANCE.fxa_mgr_finish_authentication_flow(this@FxAccountManager.handle.get(), code, state, e)
        }
        persistAccount()
        when (accountState()) {
            AccountState.Authenticated -> notifyObservers { onAuthenticated() } // Connected!
            AccountState.AuthenticationProblem -> notifyObservers { onAuthenticationProblems() } // Uh oh.
            else -> { /* Do nothing */ }
        }
    }

    fun updateProfileAsync(): Deferred<Unit?> = CoroutineScope(coroutineContext).async {
        val oldAccountState = accountState()
        val profileBuffer = rustCallWithLock { e ->
            LibAccountsFFI.INSTANCE.fxa_mgr_update_profile(this@FxAccountManager.handle.get(), e)
        }
        persistAccount()
        if (accountState() == AccountState.AuthenticationProblem &&
                oldAccountState == AccountState.Authenticated) {
            notifyObservers { onAuthenticationProblems() } // Uh oh we broke it.
        }
        profileBuffer.asCodedInputStream()?.let { // Nullable!
            try {
                val p = FxAMsgTypes.Profile.parseFrom(it)
                val profile = Profile.fromMessage(p)
                notifyObservers { onProfileUpdated(profile) }
            } finally {
                LibAccountsFFI.INSTANCE.fxa_mgr_bytebuffer_free(profileBuffer)
            }
        }
    }

    fun getProfile(): Profile? {
        val profileBuffer = rustCallWithLock { e ->
            LibAccountsFFI.INSTANCE.fxa_mgr_get_profile(this@FxAccountManager.handle.get(), e)
        }
        profileBuffer.asCodedInputStream()?.let { // Nullable!
            try {
                val p = FxAMsgTypes.Profile.parseFrom(it)
                return Profile.fromMessage(p)
            } finally {
                LibAccountsFFI.INSTANCE.fxa_mgr_bytebuffer_free(profileBuffer)
            }
        }
        return null
    }

    fun onAuthenticationErrorAsync(): Deferred<Unit> = CoroutineScope(coroutineContext).async {
        rustCallWithLock { e ->
            LibAccountsFFI.INSTANCE.fxa_mgr_on_authentication_error(this@FxAccountManager.handle.get(), e)
        }
        if (accountState() == AccountState.Authenticated) {
            notifyObservers { onAuthenticated() } // We fixed the problem, yippee.
        }
    }

    fun logoutAsync(): Deferred<Unit> = CoroutineScope(coroutineContext).async {
        rustCallWithLock { e ->
            LibAccountsFFI.INSTANCE.fxa_mgr_logout(this@FxAccountManager.handle.get(), e)
        }
        notifyObservers { onLoggedOut() }
    }

    fun accountStateAsync(): Deferred<AccountState> = CoroutineScope(coroutineContext).async {
        accountState()
    }

    fun deviceConstellation(): FxADeviceConstellation {
        return deviceConstellation
    }

    private fun accountState(): AccountState {
        val buffer = rustCallWithLock { e ->
            LibAccountsFFI.INSTANCE.fxa_mgr_account_state(this@FxAccountManager.handle.get(), e)
        }
        try {
            val msg = MsgTypes.AccountState.parseFrom(buffer.asCodedInputStream()!!)
            return AccountState.fromMessage(msg)
        } finally {
            LibAccountsFFI.INSTANCE.fxa_mgr_bytebuffer_free(buffer)
        }
    }

    private fun persistAccount() {
        try {
            val json = rustCallWithLock { e ->
                LibAccountsFFI.INSTANCE.fxa_mgr_export_persisted_state(this.handle.get(), e)
            }.getAndConsumeRustString()
            getAccountStorage().write(json)
        } catch (e: FxaException) {
            Log.e("FxAccountManager", "Error serializing the FxA state.")
        }
    }

    private fun getAccountStorage(): AccountStorage {
        return SharedPrefAccountStorage(context)
    }

    inner class FxADeviceConstellation : Observable<DeviceEventsObserver> by ObserverRegistry() {
        private val deviceObserverRegistry = ObserverRegistry<DeviceConstellationObserver>()
        fun registerDeviceObserver(
            observer: DeviceConstellationObserver,
            owner: LifecycleOwner,
            autoPause: Boolean
        ) {
            deviceObserverRegistry.register(observer, owner, autoPause)
        }

        fun state(): ConstellationState {
            val buffer = rustCallWithLock { e ->
                LibAccountsFFI.INSTANCE.fxa_mgr_get_devices(this@FxAccountManager.handle.get(), e)
            }
            try {
                val msg = FxAMsgTypes.DeviceConstellation.parseFrom(buffer.asCodedInputStream()!!)
                return ConstellationState.fromMessage(msg)
            } finally {
                LibAccountsFFI.INSTANCE.fxa_mgr_bytebuffer_free(buffer)
            }
        }

        fun refreshDevicesAsync(): Deferred<Unit> = CoroutineScope(coroutineContext).async {
            val oldAccountState = accountState()
            val devicesBuffer = rustCallWithLock { e ->
                LibAccountsFFI.INSTANCE.fxa_mgr_update_devices(this@FxAccountManager.handle.get(), e)
            }
            persistAccount()
            if (accountState() == AccountState.AuthenticationProblem &&
                    oldAccountState == AccountState.Authenticated) {
                this@FxAccountManager.notifyObservers { onAuthenticationProblems() } // Uh oh we broke it.
            }
            try {
                val msg = FxAMsgTypes.DeviceConstellation.parseFrom(devicesBuffer.asCodedInputStream()!!)
                val deviceConstellation = ConstellationState.fromMessage(msg)
                deviceObserverRegistry.notifyObservers { onDevicesUpdate(deviceConstellation) }
            } finally {
                LibAccountsFFI.INSTANCE.fxa_mgr_bytebuffer_free(devicesBuffer)
            }
        }

        fun processRawEventAsync(payload: String): Deferred<Unit> = CoroutineScope(coroutineContext).async {
            val eventsBuffer = rustCallWithLock { e ->
                LibAccountsFFI.INSTANCE.fxa_mgr_handle_push_message(this@FxAccountManager.handle.get(), payload, e)
            }
            persistAccount()
            try {
                val e = mozilla.appservices.fxaclient.MsgTypes.AccountEvents.parseFrom(eventsBuffer.asCodedInputStream()!!)
                val events = AccountEvent.fromCollectionMessage(e)
                if (events.size > 0) {
                    notifyObservers { onEvents(events.toList()) }
                }
            } finally {
                LibAccountsFFI.INSTANCE.fxa_mgr_bytebuffer_free(eventsBuffer)
            }
        }

        fun setDeviceNameAsync(name: String): Deferred<Unit> = CoroutineScope(coroutineContext).async {
            val oldAccountState = accountState()
            rustCallWithLock { e ->
                LibAccountsFFI.INSTANCE.fxa_mgr_set_device_name(this@FxAccountManager.handle.get(), name, e)
            }
            // TODO: this pattern is repeated throughout the file, we should create a helper for it.
            if (accountState() == AccountState.AuthenticationProblem &&
                    oldAccountState == AccountState.Authenticated) {
                this@FxAccountManager.notifyObservers { onAuthenticationProblems() } // Uh oh we broke it.
            }
        }

        fun setDevicePushSubscriptionAsync(subscription: Device.PushSubscription): Deferred<Unit> = CoroutineScope(coroutineContext).async {
            val oldAccountState = accountState()
            rustCallWithLock { e ->
                LibAccountsFFI.INSTANCE.fxa_mgr_set_push_subscription(this@FxAccountManager.handle.get(), subscription.endpoint, subscription.publicKey, subscription.authKey, e)
            }
            if (accountState() == AccountState.AuthenticationProblem &&
                    oldAccountState == AccountState.Authenticated) {
                this@FxAccountManager.notifyObservers { onAuthenticationProblems() } // Uh oh we broke it.
            }
        }

        fun sendEventToDeviceAsync(targetDeviceId: String, outgoingEvent: DeviceEventOutgoing): Deferred<Unit> = CoroutineScope(coroutineContext).async {
            val oldAccountState = accountState()
            when (outgoingEvent) {
                is DeviceEventOutgoing.SendTab -> {
                    rustCallWithLock { e ->
                        LibAccountsFFI.INSTANCE.fxa_mgr_send_tab(this@FxAccountManager.handle.get(), targetDeviceId, outgoingEvent.title, outgoingEvent.url, e)
                    }
                }
            }
            if (accountState() == AccountState.AuthenticationProblem &&
                    oldAccountState == AccountState.Authenticated) {
                this@FxAccountManager.notifyObservers { onAuthenticationProblems() } // Uh oh we broke it.
            }
        }

        fun pollForEventsAsync(): Deferred<Unit> = CoroutineScope(coroutineContext).async {
            // TODO: factorize with push payload handling code.
            val eventsBuffer = rustCallWithLock { e ->
                LibAccountsFFI.INSTANCE.fxa_mgr_poll_device_commands(this@FxAccountManager.handle.get(), e)
            }
            persistAccount()
            try {
                val e = mozilla.appservices.fxaclient.MsgTypes.AccountEvents.parseFrom(eventsBuffer.asCodedInputStream()!!)
                val events = AccountEvent.fromCollectionMessage(e)
                if (events.size > 0) {
                    notifyObservers { onEvents(events.toList()) }
                }
            } finally {
                LibAccountsFFI.INSTANCE.fxa_mgr_bytebuffer_free(eventsBuffer)
            }
        }
    }

    @Synchronized
    override fun close() {
        val handle = this.handle.getAndSet(0)
        if (handle != 0L) {
            rustCall { err ->
                LibAccountsFFI.INSTANCE.fxa_mgr_free(handle, err)
            }
        }
    }

    private inline fun <U> nullableRustCallWithLock(callback: (RustError.ByReference) -> U?): U? {
        return synchronized(this) {
            nullableRustCall { callback(it) }
        }
    }

    private inline fun <U> rustCallWithLock(callback: (RustError.ByReference) -> U?): U {
        return nullableRustCallWithLock(callback)!!
    }
}

private fun capabilitiesToBuffer(capabilities: Set<Device.Capability>): Pair<ByteBuffer, Int> {
    val capabilitiesBuilder = FxAMsgTypes.Capabilities.newBuilder()
    capabilities.forEach {
        when (it) {
            Device.Capability.SEND_TAB -> capabilitiesBuilder.addCapability(FxAMsgTypes.Device.Capability.SEND_TAB)
        }.exhaustive
    }
    val buf = capabilitiesBuilder.build()
    return buf.toNioDirectBuffer()
}

// In practice we usually need to be synchronized to call this safely, so it doesn't
// synchronize itself
private inline fun <U> nullableRustCall(callback: (RustError.ByReference) -> U?): U? {
    val e = RustError.ByReference()
    try {
        val ret = callback(e)
        if (e.isFailure()) {
            throw e.intoException()
        }
        return ret
    } finally {
        // This only matters if `callback` throws (or does a non-local return, which
        // we currently don't do)
        e.ensureConsumed()
    }
}

private inline fun <U> rustCall(callback: (RustError.ByReference) -> U?): U {
    return nullableRustCall(callback)!!
}

/**
 * Helper to read a null terminated String out of the Pointer and free it.
 *
 * Important: Do not use this pointer after this! For anything!
 */
internal fun Pointer.getAndConsumeRustString(): String {
    try {
        return this.getRustString()
    } finally {
        LibAccountsFFI.INSTANCE.fxa_mgr_str_free(this)
    }
}

/**
 * Helper to read a null terminated string out of the pointer.
 *
 * Important: doesn't free the pointer, use [getAndConsumeRustString] for that!
 */
internal fun Pointer.getRustString(): String {
    return this.getString(0, "utf8")
}
