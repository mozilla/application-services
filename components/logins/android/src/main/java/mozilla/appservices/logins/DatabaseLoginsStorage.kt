/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.logins

/*
 * Import some private Glean types, so that we can use them in type declarations.
 *
 * I do not like importing these private classes, but I do like the nice generic
 * code they allow me to write! By agreement with the Glean team, we must not
 * instantiate anything from these classes, and it's on us to fix any bustage
 * on version updates.
 */
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
        store.delete(id)
    }

    @Throws(LoginsApiException::class)
    suspend fun get(id: String): Login? = withContext(coroutineContext) {
        store.get(id)
    }

    @Throws(LoginsApiException::class)
    suspend fun touch(id: String): Unit = withContext(coroutineContext) {
        store.touch(id)
    }

    @Throws(LoginsApiException::class)
    suspend fun isEmpty(): Boolean = withContext(coroutineContext) {
        store.isEmpty()
    }

    @Throws(LoginsApiException::class)
    suspend fun list(): List<Login> = withContext(coroutineContext) {
        store.list()
    }

    @Throws(LoginsApiException::class)
    suspend fun hasLoginsByBaseDomain(baseDomain: String): Boolean = withContext(coroutineContext) {
        store.hasLoginsByBaseDomain(baseDomain)
    }

    @Throws(LoginsApiException::class)
    suspend fun getByBaseDomain(baseDomain: String): List<Login> = withContext(coroutineContext) {
        store.getByBaseDomain(baseDomain)
    }

    @Throws(LoginsApiException::class)
    suspend fun findLoginToUpdate(look: LoginEntry): Login? = withContext(coroutineContext) {
        store.findLoginToUpdate(look)
    }

    @Throws(LoginsApiException::class)
    suspend fun add(entry: LoginEntry): Login = withContext(coroutineContext) {
        store.add(entry)
    }

    @Throws(LoginsApiException::class)
    suspend fun update(id: String, entry: LoginEntry): Login = withContext(coroutineContext) {
        store.update(id, entry)
    }

    @Throws(LoginsApiException::class)
    suspend fun addOrUpdate(entry: LoginEntry): Login = withContext(coroutineContext) {
        store.addOrUpdate(entry)
    }

    fun registerWithSyncManager() {
        return store.registerWithSyncManager()
    }

    /**
     * Performs maintenance actions on the store
     */
    fun runMaintenance() {
        return store.runMaintenance()
    }

    @Synchronized
    @Throws(LoginsApiException::class)
    override fun close() {
        store.close()
    }

    @Throws(LoginsApiException::class)
    fun deleteUndecryptableLoginsAndRecordMetrics() {
        val result = store.deleteUndecryptableRecordsForRemoteReplacement()
        if (result.localDeleted > 0u) {
            LoginsStoreMetrics.localUndecryptableDeleted.add(result.localDeleted.toInt())
        }
        if (result.mirrorDeleted > 0u) {
            LoginsStoreMetrics.mirrorUndecryptableDeleted.add(result.mirrorDeleted.toInt())
        }
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
