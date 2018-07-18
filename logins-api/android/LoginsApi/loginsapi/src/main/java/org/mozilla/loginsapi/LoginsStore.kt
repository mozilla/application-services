package org.mozilla.loginsapi

import org.mozilla.loginsapi.rust.PasswordSyncAdapter
import android.util.Log
import com.beust.klaxon.Klaxon
import com.sun.jna.Pointer
import kotlinx.coroutines.experimental.launch
import org.mozilla.loginsapi.rust.RawLoginSyncState
import org.mozilla.loginsapi.rust.RustError
import java.io.Closeable

class RustException(msg: String): Exception(msg) {}

class LoginsStore(private var raw: RawLoginSyncState?) : Closeable {

    fun sync(): SyncResult<Unit> {
        return safeAsync { error ->
            Log.d("LoginsAPI", "sync")
            PasswordSyncAdapter.INSTANCE.sync15_passwords_sync(this.raw!!, error)
        }
    }

    fun reset(): SyncResult<Unit> {
        return safeAsync { error ->
            Log.d("LoginsAPI", "reset")
            PasswordSyncAdapter.INSTANCE.sync15_passwords_reset(this.raw!!, error)
        }
    }

    fun wipe(): SyncResult<Unit> {
        return safeAsync { error ->
            Log.d("LoginsAPI", "wipe")
            PasswordSyncAdapter.INSTANCE.sync15_passwords_wipe(this.raw!!, error)
        }
    }

    fun delete(id: String): SyncResult<Unit> {
        return safeAsync { error ->
            Log.d("LoginsAPI", "delete by id")
            PasswordSyncAdapter.INSTANCE.sync15_passwords_delete(this.raw!!, id, error)
        }
    }

    fun get(id: String): SyncResult<ServerPassword?> {
        return safeAsyncString { error ->
            PasswordSyncAdapter.INSTANCE.sync15_passwords_get_by_id(this.raw!!, id, error)
        }.then { json ->
            SyncResult.fromValue(
                    if (json == null) { null }
                    else { Klaxon().parse<ServerPassword>(json) }
            )
        }
    }

    fun touch(id: String): SyncResult<Unit> {
        return safeAsync { error ->
            Log.d("LoginsAPI", "touch by id")
            PasswordSyncAdapter.INSTANCE.sync15_passwords_touch(this.raw!!, id, error)
        }
    }

    fun list(): SyncResult<List<ServerPassword>> {
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

        fun create(databasePath: String,
                              databaseKey: String,
                              kid: String,
                              accessToken: String,
                              syncKey: String,
                              tokenserverURL: String): SyncResult<LoginsStore> {
            Log.d("API", "in the module")
            return safeAsync { error ->
                PasswordSyncAdapter.INSTANCE.sync15_passwords_state_new(
                        databasePath,
                        databaseKey,
                        kid,
                        accessToken,
                        syncKey,
                        tokenserverURL,
                        error
                )
            }.then { rawStore ->
                SyncResult.fromValue(LoginsStore(rawStore))
            }
        }

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
                    val ret = callback(e)
                    if (e.isFailure()) {
                        result.completeExceptionally(RustException(e.consumeErrorMessage()))
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

