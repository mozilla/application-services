/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.logins

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

class DatabaseLoginsStorage(dbPath: String) : AutoCloseable {
    private var store: LoginStore

    init {
        this.store = LoginStore(dbPath)
    }

    @Throws(LoginsStorageException::class)
    fun reset() {
        this.store.reset()
    }

    @Throws(LoginsStorageException::class)
    fun wipe() {
        this.store.wipe()
    }

    @Throws(LoginsStorageException::class)
    fun wipeLocal() {
        this.store.wipeLocal()
    }

    @Throws(LoginsStorageException::class)
    fun delete(id: String): Boolean {
        return writeQueryCounters.measure {
            store.delete(id)
        }
    }

    @Throws(LoginsStorageException::class)
    fun get(id: String): EncryptedLogin? {
        return readQueryCounters.measure {
            store.get(id)
        }
    }

    @Throws(LoginsStorageException::class)
    fun touch(id: String) {
        writeQueryCounters.measure {
            store.touch(id)
        }
    }

    @Throws(LoginsStorageException::class)
    fun list(): List<EncryptedLogin> {
        return readQueryCounters.measure {
            store.list()
        }
    }

    @Throws(LoginsStorageException::class)
    fun getByBaseDomain(baseDomain: String): List<EncryptedLogin> {
        return readQueryCounters.measure {
            store.getByBaseDomain(baseDomain)
        }
    }

    @Throws(LoginsStorageException::class)
    fun add(entry: LoginEntry, encryptionKey: String): EncryptedLogin {
        return writeQueryCounters.measure {
            store.add(entry, encryptionKey)
        }
    }

    @Throws(LoginsStorageException::class)
    fun importLogins(logins: List<Login>, encryptionKey: String): String {
        return writeQueryCounters.measure {
            store.importMultiple(logins, encryptionKey)
        }
    }

    @Throws(LoginsStorageException::class)
    fun update(id: String, entry: LoginEntry, encryptionKey: String): EncryptedLogin {
        return writeQueryCounters.measure {
            store.update(id, entry, encryptionKey)
        }
    }

    fun registerWithSyncManager() {
        return store.registerWithSyncManager()
    }

    @Synchronized
    @Throws(LoginsStorageException::class)
    override fun close() {
        store.close()
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
