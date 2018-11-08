/* Copyright 2018 Mozilla
 * Licensed under the Apache License, Version 2.0 (the "License"); you may not use
 * this file except in compliance with the License. You may obtain a copy of the
 * License at http://www.apache.org/licenses/LICENSE-2.0
 * Unless required by applicable law or agreed to in writing, software distributed
 * under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
 * CONDITIONS OF ANY KIND, either express or implied. See the License for the
 * specific language governing permissions and limitations under the License. */
package mozilla.appservices.logins

import com.sun.jna.Pointer
import mozilla.appservices.logins.rust.PasswordSyncAdapter
import mozilla.appservices.logins.rust.RawLoginSyncState
import mozilla.appservices.logins.rust.RustError
import java.util.concurrent.atomic.AtomicReference

/**
 * LoginsStorage implementation backed by a database.
 */
class DatabaseLoginsStorage(private val dbPath: String) : AutoCloseable, LoginsStorage {

    private var raw: AtomicReference<RawLoginSyncState?> = AtomicReference(null)

    override fun isLocked(): Boolean {
        return raw.get() == null
    }

    private fun checkUnlocked() {
        if (isLocked()) {
            throw LoginsStorageException("Using DatabaseLoginsStorage without unlocking first");
        }
    }

    @Synchronized
    override fun lock() {
        if (isLocked()) {
            throw MismatchedLockException("Lock called when we are already locked")
        }
        val raw = this.raw.getAndSet(null)
        if (raw != null) {
            PasswordSyncAdapter.INSTANCE.sync15_passwords_state_destroy(raw)
        }
    }

    @Synchronized
    override fun unlock(encryptionKey: String) {
        return rustCall {
            if (!isLocked()) {
                throw MismatchedLockException("Unlock called when we are already unlocked");
            }
            raw.set(PasswordSyncAdapter.INSTANCE.sync15_passwords_state_new(
                    dbPath,
                    encryptionKey,
                    it))
        }
    }

    override fun sync(syncInfo: SyncUnlockInfo) {
        rustCallWithLock { raw, error ->
            PasswordSyncAdapter.INSTANCE.sync15_passwords_sync(
                    raw,
                    syncInfo.kid,
                    syncInfo.fxaAccessToken,
                    syncInfo.syncKey,
                    syncInfo.tokenserverURL,
                    error
            )
        }
    }

    override fun reset() {
        rustCallWithLock { raw, error ->
            PasswordSyncAdapter.INSTANCE.sync15_passwords_reset(raw, error)
        }
    }

    override fun wipe() {
        rustCallWithLock { raw, error ->
            PasswordSyncAdapter.INSTANCE.sync15_passwords_wipe(raw, error)
        }
    }

    override fun delete(id: String): Boolean {
        return rustCallWithLock { raw, error ->
            val deleted = PasswordSyncAdapter.INSTANCE.sync15_passwords_delete(raw, id, error)
            deleted.toInt() != 0
        }
    }

    override fun get(id: String): ServerPassword? {
        val json = nullableRustCallWithLock { raw, error ->
            checkUnlocked()
            PasswordSyncAdapter.INSTANCE.sync15_passwords_get_by_id(raw, id, error)
        }?.getAndConsumeRustString()
        return json?.let { ServerPassword.fromJSON(it) }
    }

    override fun touch(id: String) {
        rustCallWithLock { raw, error ->
            PasswordSyncAdapter.INSTANCE.sync15_passwords_touch(raw, id, error)
        }
    }

    override fun list(): List<ServerPassword> {
        val json = rustCallWithLock { raw, error ->
            PasswordSyncAdapter.INSTANCE.sync15_passwords_get_all(raw, error)
        }.getAndConsumeRustString()
        return ServerPassword.fromJSONArray(json)
    }

    override fun add(login: ServerPassword): String {
        val s = login.toJSON().toString()
        return rustCallWithLock { raw, error ->
            PasswordSyncAdapter.INSTANCE.sync15_passwords_add(raw, s, error)
        }.getAndConsumeRustString()
    }

    override fun update(login: ServerPassword) {
        val s = login.toJSON().toString()
        return rustCallWithLock { raw, error ->
            PasswordSyncAdapter.INSTANCE.sync15_passwords_update(raw, s, error)
        }
    }

    @Synchronized
    override fun close() {
        val raw = this.raw.getAndSet(null)
        if (raw != null) {
            PasswordSyncAdapter.INSTANCE.sync15_passwords_state_destroy(raw)
        }
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

    private inline fun <U> nullableRustCallWithLock(callback: (RawLoginSyncState, RustError.ByReference) -> U?): U? {
        return synchronized(this) {
            checkUnlocked()
            val raw = this.raw.get()!!
            nullableRustCall { callback(raw, it) }
        }
    }

    private inline fun <U> rustCallWithLock(callback: (RawLoginSyncState, RustError.ByReference) -> U?): U {
        return nullableRustCallWithLock(callback)!!
    }
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
        PasswordSyncAdapter.INSTANCE.sync15_passwords_destroy_string(this)
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

