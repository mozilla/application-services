/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.remotetabs

import com.sun.jna.Pointer
import mozilla.appservices.remotetabs.rust.LibRemoteTabsFFI
import mozilla.appservices.remotetabs.rust.RustError
import mozilla.appservices.sync15.SyncTelemetryPing
import org.json.JSONArray
import java.util.concurrent.atomic.AtomicLong

class RemoteTabsProvider : AutoCloseable {
    private var handle: AtomicLong = AtomicLong(0)

    init {
        handle.set(rustCall { error ->
            LibRemoteTabsFFI.INSTANCE.remote_tabs_new(error)
        })
    }

    /**
     * Update our local tabs state.
     */
    fun setLocalTabs(localTabs: List<RemoteTab>) {
        val remoteTabs = JSONArray().apply {
            localTabs.forEach {
                put(it.toJSON())
            }
        }
        val remoteTabsJson = remoteTabs.toString()
        rustCallWithLock { err ->
            LibRemoteTabsFFI.INSTANCE.remote_tabs_update_local(this.handle.get(), remoteTabsJson, err)
        }
    }

    /**
     * Get the remote tabs. Might be null if we haven't synced yet.
     */
    fun getAll(): List<ClientTabs>? {
        val rustBuf = rustCallWithLock { error ->
            LibRemoteTabsFFI.INSTANCE.remote_tabs_get_all(
                this.handle.get(), error)
        }

        try {
            return rustBuf.asCodedInputStream()?.let { stream ->
                ClientTabs.fromCollectionMessage(MsgTypes.ClientsTabs.parseFrom(stream))
            }
        } finally {
            LibRemoteTabsFFI.INSTANCE.remote_tabs_destroy_bytebuffer(rustBuf)
        }
    }

    /**
     * Convenience Sync function.
     */
    fun sync(syncInfo: SyncAuthInfo, localDeviceId: String): SyncTelemetryPing {
        val json = rustCallWithLock { error ->
            LibRemoteTabsFFI.INSTANCE.remote_tabs_sync(
                    this.handle.get(),
                    syncInfo.kid,
                    syncInfo.fxaAccessToken,
                    syncInfo.syncKey,
                    syncInfo.tokenserverURL,
                    localDeviceId,
                    error
            )?.getAndConsumeRustString()
        }
        return SyncTelemetryPing.fromJSONString(json)
    }

    /**
     * Return the raw handle used to reference this RemoteTabsProvider.
     *
     * Generally should only be used to pass the handle into `SyncManager.setTabs`
     */
    fun getHandle(): Long {
        return this.handle.get()
    }

    @Synchronized
    override fun close() {
        val handle = this.handle.getAndSet(0L)
        if (handle != 0L) {
            rustCall { error ->
                LibRemoteTabsFFI.INSTANCE.remote_tabs_destroy(handle, error)
            }
        }
    }

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

    private inline fun <U> nullableRustCallWithLock(callback: (RustError.ByReference) -> U?): U? {
        return synchronized(this) {
            nullableRustCall { callback(it) }
        }
    }

    private inline fun <U> rustCallWithLock(callback: (RustError.ByReference) -> U?): U {
        return nullableRustCallWithLock(callback)!!
    }
}

/**
 * A class for providing the auth-related information needed to sync.
 */
data class SyncAuthInfo(
    val kid: String,
    val fxaAccessToken: String,
    val syncKey: String,
    val tokenserverURL: String
)

/**
 * Helper to read a null terminated String out of the Pointer and free it.
 *
 * Important: Do not use this pointer after this! For anything!
 */
internal fun Pointer.getAndConsumeRustString(): String {
    try {
        return this.getRustString()
    } finally {
        LibRemoteTabsFFI.INSTANCE.remote_tabs_destroy_string(this)
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
