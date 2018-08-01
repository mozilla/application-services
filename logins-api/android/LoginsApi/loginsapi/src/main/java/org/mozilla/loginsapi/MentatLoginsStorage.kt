package org.mozilla.loginsapi

import org.mozilla.loginsapi.rust.PasswordSyncAdapter
import android.util.Log
import com.beust.klaxon.Klaxon
import com.sun.jna.Pointer
import kotlinx.coroutines.experimental.launch
import org.mozilla.loginsapi.rust.RawLoginSyncState
import org.mozilla.loginsapi.rust.RustError
import java.io.Closeable

class MentatLoginsStorage(private val dbPath: String) : Closeable, LoginsStorage {

    private var raw: RawLoginSyncState? = null;

    override fun isLocked(): SyncResult<Boolean> {
        return safeAsync {
            // Run inside a safeAsync block to be sure that all pending operations have finished.
            raw != null
        }
    }

    override fun lock(): SyncResult<Unit> {
        return safeAsync {
            Log.d("LoginsAPI", "locking!");
            if (raw == null) {
                throw MismatchedLockException("Lock called when we are already locked")
            }
            // Free the sync state object
            var raw = this.raw;
            this.raw = null;
            if (raw != null) {
                PasswordSyncAdapter.INSTANCE.sync15_passwords_state_destroy(raw)
            }
        }
    }

    override fun unlock(encryptionKey: String, syncInfo: SyncUnlockInfo): SyncResult<Unit> {
        return safeAsync { error ->
            Log.d("LoginsAPI", "unlock");
            if (raw != null) {
                throw MismatchedLockException("Unlock called when we are already unlocked");
            }
            raw = PasswordSyncAdapter.INSTANCE.sync15_passwords_state_new(
                    dbPath,
                    encryptionKey,
                    syncInfo.kid,
                    syncInfo.fxaAccessToken,
                    syncInfo.syncKey,
                    syncInfo.tokenserverBaseURL,
                    error
            )
        }
    }

    override fun sync(): SyncResult<Unit> {
        return safeAsync { error ->
            Log.d("LoginsAPI", "sync")
            PasswordSyncAdapter.INSTANCE.sync15_passwords_sync(this.raw!!, error)
        }
    }

    override fun reset(): SyncResult<Unit> {
        return safeAsync { error ->
            Log.d("LoginsAPI", "reset")
            PasswordSyncAdapter.INSTANCE.sync15_passwords_reset(this.raw!!, error)
        }
    }

    override fun wipe(): SyncResult<Unit> {
        return safeAsync { error ->
            Log.d("LoginsAPI", "wipe")
            PasswordSyncAdapter.INSTANCE.sync15_passwords_wipe(this.raw!!, error)
        }
    }

    override fun delete(id: String): SyncResult<Unit> {
        return safeAsync { error ->
            Log.d("LoginsAPI", "delete by id")
            PasswordSyncAdapter.INSTANCE.sync15_passwords_delete(this.raw!!, id, error)
        }
    }

    override fun get(id: String): SyncResult<ServerPassword?> {
        return safeAsyncString { error ->
            PasswordSyncAdapter.INSTANCE.sync15_passwords_get_by_id(this.raw!!, id, error)
        }.then { json ->
            SyncResult.fromValue(
                    if (json == null) { null }
                    else { Klaxon().parse<ServerPassword>(json) }
            )
        }
    }

    override fun touch(id: String): SyncResult<Unit> {
        return safeAsync { error ->
            Log.d("LoginsAPI", "touch by id")
            PasswordSyncAdapter.INSTANCE.sync15_passwords_touch(this.raw!!, id, error)
        }
    }

    override fun list(): SyncResult<List<ServerPassword>> {
        return safeAsyncString {
            Log.d("LoginsAPI", "list all")
            PasswordSyncAdapter.INSTANCE.sync15_passwords_get_all(this.raw!!, it)
        }.then { json ->
            Log.d("Logins", "got list: " + json);
            SyncResult.fromValue(Klaxon().parseArray<ServerPassword>(json!!)!!)
        }
    }

    override fun close() {
        synchronized(PasswordSyncAdapter.INSTANCE) {
            var raw = this.raw;
            this.raw = null;
            if (raw != null) {
                PasswordSyncAdapter.INSTANCE.sync15_passwords_state_destroy(raw)
            }
        }
    }

    // This says it's unused but apparently this is how you add a finalizer in kotlin.
    // No override or anything
    fun finalize() {
        this.close()
    }

    companion object {

        internal fun getAndConsumeString(p: Pointer?): String? {
            if (p == null) {
                return null;
            }
            try {
                return p.getString(0, "utf8");
            } finally {
                PasswordSyncAdapter.INSTANCE.destroy_c_char(p);
            }
        }

        internal fun <U> safeAsync(callback: (RustError.ByReference) -> U): SyncResult<U> {
            val result = SyncResult<U>()
            val e = RustError.ByReference()
            launch {
                synchronized(PasswordSyncAdapter.INSTANCE) {
                    val ret: U;
                    try {
                        ret = callback(e)
                    } catch (e: Exception) {
                        result.completeExceptionally(e)
                        return@launch
                    }
                    if (e.isFailure()) {
                        result.completeExceptionally(LoginsStorageException(e.consumeErrorMessage()))
                    } else {
                        result.complete(ret)
                    }
                }
            }
            return result
        }

        internal fun safeAsyncString(callback: (RustError.ByReference) -> Pointer?): SyncResult<String?> {
            return safeAsync { e -> getAndConsumeString(callback(e)) }
        }
    }
}

