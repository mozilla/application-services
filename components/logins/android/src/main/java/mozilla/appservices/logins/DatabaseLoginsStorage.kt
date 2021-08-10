/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.logins

import java.util.concurrent.atomic.AtomicReference
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
 * An artifact of the uniffi conversion - a thin-ish wrapper around a
   LoginStore.
 */
class DatabaseLoginsStorage(private val dbPath: String) : AutoCloseable {
    private var store: AtomicReference<LoginStore> = AtomicReference()

    fun isLocked(): Boolean {
        return this.store.get() == null
    }

    private fun checkUnlocked(): LoginStore {
        val store = this.store.get() ?: throw LoginsStorageException.UnexpectedLoginsStorageException() // ("Using DatabaseLoginsStorage without unlocking first")
        return store
    }

    @Synchronized
    @Throws(LoginsStorageException::class)
    fun lock() {
        val store = this.store.getAndSet(null)
        if (store == null) {
            throw LoginsStorageException.MismatchedLock() // ("Lock called when we are already locked")
        }
        store.destroy()
    }

    @Synchronized
    @Throws(LoginsStorageException::class)
    fun unlock(encryptionKey: String) {
        return unlockCounters.measure {
            val store = LoginStore(dbPath, encryptionKey)
            if (this.store.getAndSet(store) != null) {
                // this seems wrong?
                throw LoginsStorageException.MismatchedLock() // ("Unlock called when we are already unlocked")
            }
        }
    }

    @Synchronized
    @Throws(LoginsStorageException::class)
    fun ensureUnlocked(encryptionKey: String) {
        if (isLocked()) {
            this.unlock(encryptionKey)
        }
    }

    @Synchronized
    fun ensureLocked() {
        if (!isLocked()) {
            this.lock()
        }
    }

    @Throws(LoginsStorageException::class)
    fun reset() {
        this.checkUnlocked().reset()
    }

    @Throws(LoginsStorageException::class)
    fun wipe() {
        this.checkUnlocked().wipe()
    }

    @Throws(LoginsStorageException::class)
    fun wipeLocal() {
        this.checkUnlocked().wipeLocal()
    }

    @Throws(LoginsStorageException::class)
    fun delete(id: String): Boolean {
        return writeQueryCounters.measure {
            checkUnlocked().delete(id)
        }
    }

    @Throws(LoginsStorageException::class)
    fun get(id: String): Login? {
        return readQueryCounters.measure {
            checkUnlocked().get(id)
        }
    }

    @Throws(LoginsStorageException::class)
    fun touch(id: String) {
        writeQueryCounters.measure {
            checkUnlocked().touch(id)
        }
    }

    @Throws(LoginsStorageException::class)
    fun list(): List<Login> {
        return readQueryCounters.measure {
            checkUnlocked().list()
        }
    }

    @Throws(LoginsStorageException::class)
    fun getByBaseDomain(baseDomain: String): List<Login> {
        return readQueryCounters.measure {
            checkUnlocked().getByBaseDomain(baseDomain)
        }
    }

    @Throws(LoginsStorageException::class)
    fun add(login: Login): String {
        return writeQueryCounters.measure {
            checkUnlocked().add(login)
        }
    }

    @Throws(LoginsStorageException::class)
    fun importLogins(logins: List<Login>): String {
        return writeQueryCounters.measure {
            checkUnlocked().importMultiple(logins)
        }
    }

    @Throws(LoginsStorageException::class)
    fun update(login: Login) {
        return writeQueryCounters.measure {
            checkUnlocked().update(login)
        }
    }

    @Synchronized
    @Throws(LoginsStorageException::class)
    fun potentialDupesIgnoringUsername(login: Login): List<Login> {
        return readQueryCounters.measure {
            checkUnlocked().potentialDupesIgnoringUsername(login)
        }
    }

    @Throws(LoginsStorageException.InvalidRecord::class)
    fun ensureValid(login: Login) {
        readQueryCounters.measureIgnoring({ e -> e is LoginsStorageException.InvalidRecord }) {
            checkUnlocked().checkValidWithNoDupes(login)
        }
    }

    @Throws(LoginsStorageException::class)
    fun rekeyDatabase(newEncryptionKey: String) {
        return checkUnlocked().rekeyDatabase(newEncryptionKey)
    }

    fun registerWithSyncManager() {
        return checkUnlocked().registerWithSyncManager()
    }

    @Synchronized
    @Throws(LoginsStorageException::class)
    override fun close() {
        this.store.getAndSet(null)?.destroy()
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
                is LoginsStorageException.MismatchedLock -> {
                    errCount["mismatched_lock"].add()
                }
                is LoginsStorageException.NoSuchRecord -> {
                    errCount["no_such_record"].add()
                }
                is LoginsStorageException.IdCollision -> {
                    errCount["id_collision"].add()
                }
                is LoginsStorageException.InvalidKey -> {
                    errCount["invalid_key"].add()
                }
                is LoginsStorageException.Interrupted -> {
                    errCount["interrupted"].add()
                }
                is LoginsStorageException.InvalidRecord -> {
                    errCount["invalid_record"].add()
                }
                is LoginsStorageException -> {
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
