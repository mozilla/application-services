/* Copyright 2018 Mozilla
 * Licensed under the Apache License, Version 2.0 (the "License"); you may not use
 * this file except in compliance with the License. You may obtain a copy of the
 * License at http://www.apache.org/licenses/LICENSE-2.0
 * Unless required by applicable law or agreed to in writing, software distributed
 * under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
 * CONDITIONS OF ANY KIND, either express or implied. See the License for the
 * specific language governing permissions and limitations under the License. */
package org.mozilla.sync15.logins

import android.util.Log
import com.sun.jna.Pointer
import kotlinx.coroutines.experimental.launch
import org.mozilla.sync15.logins.rust.PasswordSyncAdapter
import org.mozilla.sync15.logins.rust.RawLoginSyncState
import org.mozilla.sync15.logins.rust.RustError
import java.io.Closeable

/**
 * LoginsStorage implementation backed by a database.
 */
class DatabaseLoginsStorage(private val dbPath: String) : Closeable, LoginsStorage {

    private var raw: RawLoginSyncState? = null;

    override fun isLocked(): SyncResult<Boolean> {
        return safeAsync {
            // Run inside a safeAsync block to be sure that all pending operations have finished.
            raw == null
        }
    }

    private fun checkUnlocked() {
        if (raw == null) {
            throw LoginsStorageException("Using DatabaseLoginsStorage without unlocking first");
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

    override fun unlock(encryptionKey: String): SyncResult<Unit> {
        return safeAsync { error ->
            Log.d("LoginsAPI", "unlock");
            if (raw != null) {
                throw MismatchedLockException("Unlock called when we are already unlocked");
            }
            raw = PasswordSyncAdapter.INSTANCE.sync15_passwords_state_new(
                    dbPath,
                    encryptionKey,
                    error
            )
        }
    }

    override fun sync(syncInfo: SyncUnlockInfo): SyncResult<Unit> {
        return safeAsync { error ->
            Log.d("LoginsAPI", "sync")
            checkUnlocked()
            PasswordSyncAdapter.INSTANCE.sync15_passwords_sync(this.raw!!,
                    syncInfo.kid,
                    syncInfo.fxaAccessToken,
                    syncInfo.syncKey,
                    syncInfo.tokenserverURL,
                    error)
        }
    }

    override fun reset(): SyncResult<Unit> {
        return safeAsync { error ->
            Log.d("LoginsAPI", "reset")
            checkUnlocked()
            PasswordSyncAdapter.INSTANCE.sync15_passwords_reset(this.raw!!, error)
        }
    }

    override fun wipe(): SyncResult<Unit> {
        return safeAsync { error ->
            Log.d("LoginsAPI", "wipe")
            checkUnlocked()
            PasswordSyncAdapter.INSTANCE.sync15_passwords_wipe(this.raw!!, error)
        }
    }

    override fun delete(id: String): SyncResult<Boolean> {
        return safeAsync { error ->
            Log.d("LoginsAPI", "delete by id")
            checkUnlocked()
            val deleted = PasswordSyncAdapter.INSTANCE.sync15_passwords_delete(this.raw!!, id, error)
            deleted.toInt() != 0
        }
    }

    override fun get(id: String): SyncResult<ServerPassword?> {
        return safeAsyncString { error ->
            checkUnlocked()
            PasswordSyncAdapter.INSTANCE.sync15_passwords_get_by_id(this.raw!!, id, error)
        }.then { json ->
            SyncResult.fromValue(
                    if (json == null) {
                        null
                    } else {
                        ServerPassword.fromJSON(json)
                    }
            )
        }
    }

    override fun touch(id: String): SyncResult<Unit> {
        return safeAsync { error ->
            Log.d("LoginsAPI", "touch by id")
            checkUnlocked()
            PasswordSyncAdapter.INSTANCE.sync15_passwords_touch(this.raw!!, id, error)
        }
    }

    override fun list(): SyncResult<List<ServerPassword>> {
        return safeAsyncString {
            Log.d("LoginsAPI", "list all")
            checkUnlocked()
            PasswordSyncAdapter.INSTANCE.sync15_passwords_get_all(this.raw!!, it)
        }.then { json ->
            Log.d("Logins", "got list: " + json)
            checkUnlocked()
            SyncResult.fromValue(ServerPassword.fromJSONArray(json!!))
        }
    }

    override fun add(login: ServerPassword): SyncResult<String> {
        return safeAsyncString {
            val s = login.toJSON().toString()
            checkUnlocked()
            PasswordSyncAdapter.INSTANCE.sync15_passwords_add(this.raw!!, s, it)
        }.then {
            SyncResult.fromValue(it!!)
        }
    }

    override fun update(login: ServerPassword): SyncResult<Unit> {
        return safeAsync {
            val s = login.toJSON().toString()
            checkUnlocked()
            PasswordSyncAdapter.INSTANCE.sync15_passwords_update(this.raw!!, s, it)
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
                PasswordSyncAdapter.INSTANCE.sync15_passwords_destroy_string(p);
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
                        result.completeExceptionally(e.intoException())
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

