/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.logins

import com.sun.jna.Native
import com.sun.jna.Pointer
//import mozilla.appservices.logins.rust.PasswordSyncAdapter
//import mozilla.appservices.logins.rust.RustError
import mozilla.appservices.support.native.toNioDirectBuffer
import mozilla.appservices.sync15.SyncTelemetryPing
import java.util.concurrent.atomic.AtomicLong
import org.json.JSONObject
import org.mozilla.appservices.logins.GleanMetrics.LoginsStore as LoginsStoreMetrics

/**
 * Import some private Glean types, so that we can use them in type declarations.
 *
 * I do not like importing these private classes, but I do like the nice generic
 * code they allow me to write! By agreement with the Glean team, we must not
 * instantiate anything from these classes, and it's on us to fix any bustage
 * on version updates.
 */
import mozilla.components.service.glean.private.CounterMetricType
import mozilla.components.service.glean.private.LabeledMetricType

/**
 * LoginsStorage implementation backed by a database.
 */
class DatabaseLoginsStorage(private val dbPath: String) : AutoCloseable, LoginsStorage {
    private lateinit var store: PasswordStore
    private var raw: AtomicLong = AtomicLong(0)

    override fun isLocked(): Boolean {
        return raw.get() == 0L
    }

    private fun checkUnlocked(): Long {
        val handle = raw.get()
        if (handle == 0L) {
            throw LoginsStorageErrorException("Using DatabaseLoginsStorage without unlocking first")
        }
        return handle
    }

    /**
     * Return the raw handle used to reference this logins database.
     *
     * Generally should only be used to pass the handle into `SyncManager.setLogins`.
     *
     * Note: handles do not remain valid after locking / unlocking the logins database.
     */
    override fun getHandle(): Long {
        return this.raw.get()
    }

    @Synchronized
    @Throws(LoginsStorageErrorException::class)
    override fun lock() {
        val raw = this.raw.getAndSet(0)
        if (raw == 0L) {
            throw LoginsStorageErrorException.MismatchedLock("Lock called when we are already locked")
        }
        rustCall { _ ->
            this.store.destroy()
        }
    }

    @Synchronized
    @Throws(LoginsStorageErrorException::class)
    override fun unlock(encryptionKey: String) {
        return unlockCounters.measure {
            rustCall {
                if (!isLocked()) {
                    throw LoginsStorageErrorException.MismatchedLock("Unlock called when we are already unlocked")
                }
                this.store = PasswordStore(dbPath, encryptionKey)
                //raw.set(this.store)
            }
        }
    }

    @Synchronized
    @Throws(LoginsStorageErrorException::class)
    override fun ensureUnlocked(encryptionKey: String) {
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

    // @Throws(LoginsStorageErrorException::class)
    // override fun sync(syncInfo: SyncUnlockInfo): SyncTelemetryPing {
    //     val json = rustCallWithLock { _, _ ->
    //         PasswordSyncAdapter.INSTANCE.sync15_passwords_sync(
    //                 raw,
    //                 syncInfo.kid,
    //                 syncInfo.fxaAccessToken,
    //                 syncInfo.syncKey,
    //                 syncInfo.tokenserverURL,
    //                 error
    //         )?.getAndConsumeRustString()
    //     }
    //     return SyncTelemetryPing.fromJSONString(json)
    // }

    @Throws(LoginsStorageErrorException::class)
    override fun reset() {
        rustCallWithLock { _, _ ->
            this.store.reset()
        }
    }

    @Throws(LoginsStorageErrorException::class)
    override fun wipe() {
        rustCallWithLock { _, _ ->
            this.store.wipe()
        }
    }

    @Throws(LoginsStorageErrorException::class)
    override fun wipeLocal() {
        rustCallWithLock { _, _ ->
            this.store.wipeLocal()
        }
    }

    @Throws(LoginsStorageErrorException::class)
    override fun delete(id: String): Boolean {
        return writeQueryCounters.measure {
            rustCallWithLock { _, _ ->
                this.store.delete(id)
            }
        }
    }

    @Throws(LoginsStorageErrorException::class)
    override fun get(id: String): LoginRecord? {
        return readQueryCounters.measure {
            rustCallWithLock { _, _ ->
                this.store.get(id)
            }
        }
    }

    @Throws(LoginsStorageErrorException::class)
    override fun touch(id: String) {
        writeQueryCounters.measure {
            rustCallWithLock { _, _ ->
                this.store.touch(id)
            }
        }
    }

    @Throws(LoginsStorageErrorException::class)
    override fun list(): List<LoginRecord> {
        return readQueryCounters.measure {
            rustCallWithLock { _, _ ->
                this.store.list()
            }
        }
    }

    @Throws(LoginsStorageErrorException::class)
    override fun getByBaseDomain(baseDomain: String): List<LoginRecord> {
        return readQueryCounters.measure {
            rustCallWithLock { _, _ ->
               this.store.getByBaseDomain(baseDomain)
            }
        }
    }

    @Throws(LoginsStorageErrorException::class)
    override fun add(login: LoginRecord): String {
        return writeQueryCounters.measure {
            rustCallWithLock { _, _ ->
                this.store.add(login)
            }
        }
    }

    @Throws(LoginsStorageErrorException::class)
    override fun importLogins(logins: List<LoginRecord>): MigrationMetrics {
        return writeQueryCounters.measure {
             rustCallWithLock { _, _ ->
                this.store.importMultiple(logins)
             }
        }
    }

    @Throws(LoginsStorageErrorException::class)
    override fun update(login: LoginRecord) {
        return writeQueryCounters.measure {
            rustCallWithLock { _, _ ->
                this.store.update(login)
            }
        }
    }

    @Synchronized
    @Throws(LoginsStorageErrorException::class)
    override fun potentialDupesIgnoringUsername(login: LoginRecord): List<LoginRecord> {
        return readQueryCounters.measure {
            rustCallWithLock { _, _ ->
                this.store.potentialDupesIgnoringUsername(login)
            }
        }
    }

    @Throws(LoginsStorageErrorException.InvalidRecord::class)
    override fun ensureValid(login: LoginRecord) {
        readQueryCounters.measureIgnoring({ e -> e is LoginsStorageErrorException.InvalidRecord }) {
            rustCallWithLock { _, _ ->
                this.store.checkValidWithNoDupes(login)
            }
        }
    }

    @Throws(LoginsStorageErrorException::class)
    override fun rekeyDatabase(newEncryptionKey: String) {
        return rustCallWithLock { _, _ ->
            this.store.rekeyDatabase(newEncryptionKey)
        }
    }

    @Synchronized
    @Throws(LoginsStorageErrorException::class)
    override fun close() {
        val handle = this.raw.getAndSet(0)
        if (handle != 0L) {
            rustCall { _ ->
                this.store.destroy()
            }
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

    private inline fun <U> nullableRustCallWithLock(callback: (Long, RustError.ByReference) -> U?): U? {
        return synchronized(this) {
            val handle = checkUnlocked()
            nullableRustCall { callback(handle, it) }
        }
    }

    private inline fun <U> rustCallWithLock(callback: (Long, RustError.ByReference) -> U?): U {
        return nullableRustCallWithLock(callback)!!
    }

    private val unlockCounters: LoginsStoreCounterMetrics by lazy {
        LoginsStoreCounterMetrics(
            LoginsStoreMetrics.unlockCount,
            LoginsStoreMetrics.unlockErrorCount
        )
    }

    private val readQueryCounters: LoginsStoreCounterMetrics by lazy {
        LoginsStoreCounterMetrics(
            LoginsStoreMetrics.readQueryCount,
            LoginsStoreMetrics.readQueryErrorCount
        )
    }

    private val writeQueryCounters: LoginsStoreCounterMetrics by lazy {
        LoginsStoreCounterMetrics(
            LoginsStoreMetrics.writeQueryCount,
            LoginsStoreMetrics.writeQueryErrorCount
        )
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
        //PasswordSyncAdapter.INSTANCE.sync15_passwords_destroy_string(this)
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

/**
 * A helper class for gathering basic count metrics on different kinds of LoginsStore operation.
 *
 * For each type of operation, we want to measure:
 *    - total count of operations performed
 *    - count of operations that produced an error, labeled by type
 *
 * This is a convenince wrapper to measure the two in one shot.
 */
class LoginsStoreCounterMetrics(
    val count: CounterMetricType,
    val errCount: LabeledMetricType<CounterMetricType>
) {
    inline fun <U> measure(callback: () -> U): U {
        return measureIgnoring({ false }, callback)
    }

    @Suppress("ComplexMethod", "TooGenericExceptionCaught")
    inline fun <U> measureIgnoring(
        shouldIgnore: (Exception) -> Boolean,
        callback: () -> U
    ): U {
        count.add()
        try {
            return callback()
        } catch (e: Exception) {
            if (shouldIgnore(e)) {
                throw e
            }
            when (e) {
                is LoginsStorageErrorException.MismatchedLock -> {
                    errCount["mismatched_lock"].add()
                }
                is LoginsStorageErrorException.NoSuchRecord -> {
                    errCount["no_such_record"].add()
                }
                is LoginsStorageErrorException.IdCollision -> {
                    errCount["id_collision"].add()
                }
                is LoginsStorageErrorException.InvalidKey -> {
                    errCount["invalid_key"].add()
                }
                is LoginsStorageErrorException.Interrupted -> {
                    errCount["interrupted"].add()
                }
                is LoginsStorageErrorException.InvalidRecord -> {
                    errCount["invalid_record"].add()
                }
                is LoginsStorageErrorException -> {
                    errCount["storage_error"].add()
                }
                else -> {
                    errCount["__other__"].add()
                }
            }
            throw e
        }
    }
}
