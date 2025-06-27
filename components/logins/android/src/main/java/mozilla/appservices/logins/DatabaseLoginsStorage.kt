/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.logins
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import mozilla.telemetry.glean.private.CounterMetricType
import mozilla.telemetry.glean.private.LabeledMetricType
import kotlin.coroutines.CoroutineContext
import org.mozilla.appservices.logins.GleanMetrics.LoginsStore as LoginsStoreMetrics

/**
 * An artifact of the uniffi conversion - a thin-ish wrapper around a
 * LoginStore.
 */

class DatabaseLoginsStorage(
    dbPath: String,
    keyManager: KeyManager,
    private val coroutineContext: CoroutineContext = Dispatchers.IO,
) : AutoCloseable {
    private var store: LoginStore

    init {
        val encdec = createManagedEncdec(keyManager)
        this.store = LoginStore(dbPath, encdec)
    }

    @Throws(LoginsApiException::class)
    suspend fun reset(): Unit = withContext(coroutineContext) {
        store.reset()
    }

    @Throws(LoginsApiException::class)
    suspend fun wipeLocal(): Unit = withContext(coroutineContext) {
        store.wipeLocal()
    }

    @Throws(LoginsApiException::class)
    suspend fun delete(id: String): Boolean = withContext(coroutineContext) {
        writeQueryCounters.measure {
            store.delete(id)
        }
    }

    @Throws(LoginsApiException::class)
    suspend fun get(id: String): Login? = withContext(coroutineContext) {
        readQueryCounters.measure {
            store.get(id)
        }
    }

    @Throws(LoginsApiException::class)
    suspend fun touch(id: String): Unit = withContext(coroutineContext) {
        writeQueryCounters.measure {
            store.touch(id)
        }
    }

    @Throws(LoginsApiException::class)
    suspend fun isEmpty(): Boolean = withContext(coroutineContext) {
        readQueryCounters.measure {
            store.isEmpty()
        }
    }

    @Throws(LoginsApiException::class)
    suspend fun list(): List<Login> = withContext(coroutineContext) {
        readQueryCounters.measure {
            store.list()
        }
    }

    @Throws(LoginsApiException::class)
    suspend fun hasLoginsByBaseDomain(baseDomain: String): Boolean = withContext(coroutineContext) {
        readQueryCounters.measure {
            store.hasLoginsByBaseDomain(baseDomain)
        }
    }

    @Throws(LoginsApiException::class)
    suspend fun getByBaseDomain(baseDomain: String): List<Login> = withContext(coroutineContext) {
        readQueryCounters.measure {
            store.getByBaseDomain(baseDomain)
        }
    }

    @Throws(LoginsApiException::class)
    suspend fun findLoginToUpdate(look: LoginEntry): Login? = withContext(coroutineContext) {
        readQueryCounters.measure {
            store.findLoginToUpdate(look)
        }
    }

    @Throws(LoginsApiException::class)
    suspend fun add(entry: LoginEntry): Login = withContext(coroutineContext) {
        writeQueryCounters.measure {
            store.add(entry)
        }
    }

    @Throws(LoginsApiException::class)
    suspend fun update(id: String, entry: LoginEntry): Login = withContext(coroutineContext) {
        writeQueryCounters.measure {
            store.update(id, entry)
        }
    }

    @Throws(LoginsApiException::class)
    suspend fun addOrUpdate(entry: LoginEntry): Login = withContext(coroutineContext) {
        writeQueryCounters.measure {
            store.addOrUpdate(entry)
        }
    }

    fun registerWithSyncManager() {
        store.registerWithSyncManager()
    }

    @Synchronized
    @Throws(LoginsApiException::class)
    override fun close() {
        store.close()
    }

    private val readQueryCounters: LoginsStoreCounterMetrics by lazy {
        LoginsStoreCounterMetrics(
            LoginsStoreMetrics.readQueryCount,
            LoginsStoreMetrics.readQueryErrorCount,
        )
    }

    private val writeQueryCounters: LoginsStoreCounterMetrics by lazy {
        LoginsStoreCounterMetrics(
            LoginsStoreMetrics.writeQueryCount,
            LoginsStoreMetrics.writeQueryErrorCount,
        )
    }
}

enum class KeyRegenerationEventReason {
    Lost, Corrupt, Other,
}

fun recordKeyRegenerationEvent(reason: KeyRegenerationEventReason) {
    // Avoid the deprecation warning when calling  `record()` without the optional EventExtras param
    @Suppress("DEPRECATION")
    when (reason) {
        KeyRegenerationEventReason.Lost -> LoginsStoreMetrics.keyRegeneratedLost.record()
        KeyRegenerationEventReason.Corrupt -> LoginsStoreMetrics.keyRegeneratedCorrupt.record()
        KeyRegenerationEventReason.Other -> LoginsStoreMetrics.keyRegeneratedOther.record()
    }
}

/**
 * A helper class for gathering basic count metrics on different kinds of LoginsStore operation.
 *
 * For each type of operation, we want to measure:
 *    - total count of operations performed
 *    - count of operations that produced an error, labeled by type
 *
 * This is a convenience wrapper to measure the two in one shot.
 */
class LoginsStoreCounterMetrics(
    val count: CounterMetricType,
    val errCount: LabeledMetricType<CounterMetricType>,
) {
    inline fun <U> measure(callback: () -> U): U {
        return measureIgnoring({ false }, callback)
    }

    @Suppress("ComplexMethod", "TooGenericExceptionCaught")
    inline fun <U> measureIgnoring(
        shouldIgnore: (Exception) -> Boolean,
        callback: () -> U,
    ): U {
        count.add()
        try {
            return callback()
        } catch (e: Exception) {
            if (shouldIgnore(e)) {
                throw e
            }
            when (e) {
                is LoginsApiException.NoSuchRecord -> {
                    errCount["no_such_record"].add()
                }
                is LoginsApiException.Interrupted -> {
                    errCount["interrupted"].add()
                }
                is LoginsApiException.InvalidRecord -> {
                    errCount["invalid_record"].add()
                }
                is LoginsApiException -> {
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
