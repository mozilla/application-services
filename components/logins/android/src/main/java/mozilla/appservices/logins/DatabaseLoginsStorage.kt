/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

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
    @Throws(LoginsStorageException::class)
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
    @Throws(LoginsStorageException::class)
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

    @Synchronized
    @Throws(LoginsStorageException::class)
    override fun unlock(encryptionKey: ByteArray) {
        return rustCall {
            if (!isLocked()) {
                throw MismatchedLockException("Unlock called when we are already unlocked");
            }
            raw.set(PasswordSyncAdapter.INSTANCE.sync15_passwords_state_new_with_hex_key(
                    dbPath,
                    encryptionKey,
                    encryptionKey.size,
                    it))
        }
    }

    @Synchronized
    @Throws(LoginsStorageException::class)
    override fun ensureUnlocked(encryptionKey: String) {
        if (isLocked()) {
            this.unlock(encryptionKey)
        }
    }

    @Synchronized
    @Throws(LoginsStorageException::class)
    override fun ensureUnlocked(encryptionKey: ByteArray) {
        if (isLocked()) {
            this.unlock(encryptionKey)
        }
    }

    @Synchronized
    override fun ensureLocked() {
        if (!isLocked()) {
            this.lock()
        }
    }

    @Throws(LoginsStorageException::class)
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

    @Throws(LoginsStorageException::class)
    override fun reset() {
        rustCallWithLock { raw, error ->
            PasswordSyncAdapter.INSTANCE.sync15_passwords_reset(raw, error)
        }
    }

    @Throws(LoginsStorageException::class)
    override fun wipe() {
        rustCallWithLock { raw, error ->
            PasswordSyncAdapter.INSTANCE.sync15_passwords_wipe(raw, error)
        }
    }

    @Throws(LoginsStorageException::class)
    override fun wipeLocal() {
        rustCallWithLock { raw, error ->
            PasswordSyncAdapter.INSTANCE.sync15_passwords_wipe_local(raw, error)
        }
    }

    @Throws(LoginsStorageException::class)
    override fun delete(id: String): Boolean {
        return rustCallWithLock { raw, error ->
            val deleted = PasswordSyncAdapter.INSTANCE.sync15_passwords_delete(raw, id, error)
            deleted.toInt() != 0
        }
    }

    @Throws(LoginsStorageException::class)
    override fun get(id: String): ServerPassword? {
        val json = nullableRustCallWithLock { raw, error ->
            checkUnlocked()
            PasswordSyncAdapter.INSTANCE.sync15_passwords_get_by_id(raw, id, error)
        }?.getAndConsumeRustString()
        return json?.let { ServerPassword.fromJSON(it) }
    }

    @Throws(LoginsStorageException::class)
    override fun touch(id: String) {
        rustCallWithLock { raw, error ->
            PasswordSyncAdapter.INSTANCE.sync15_passwords_touch(raw, id, error)
        }
    }

    @Throws(LoginsStorageException::class)
    override fun list(): List<ServerPassword> {
        val json = rustCallWithLock { raw, error ->
            PasswordSyncAdapter.INSTANCE.sync15_passwords_get_all(raw, error)
        }.getAndConsumeRustString()
        return ServerPassword.fromJSONArray(json)
    }

    @Throws(LoginsStorageException::class)
    override fun add(login: ServerPassword): String {
        val s = login.toJSON().toString()
        return rustCallWithLock { raw, error ->
            PasswordSyncAdapter.INSTANCE.sync15_passwords_add(raw, s, error)
        }.getAndConsumeRustString()
    }

    @Throws(LoginsStorageException::class)
    override fun update(login: ServerPassword) {
        val s = login.toJSON().toString()
        return rustCallWithLock { raw, error ->
            PasswordSyncAdapter.INSTANCE.sync15_passwords_update(raw, s, error)
        }
    }

    @Synchronized
    @Throws(LoginsStorageException::class)
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

