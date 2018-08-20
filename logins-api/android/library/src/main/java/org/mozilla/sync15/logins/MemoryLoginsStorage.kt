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
import kotlinx.coroutines.experimental.launch
import java.io.Closeable

private enum class LoginsStorageState {
    Unlocked,
    Locked,
    Closed,
}

class MemoryLoginsStorage(private var list: List<ServerPassword>) : Closeable, LoginsStorage {
    private var state: LoginsStorageState = LoginsStorageState.Locked;

    init {
        // Check that the list we were given as input doesn't have any duplicated IDs.
        val ids = HashSet<String>(list.map { it.id })
        if (ids.size != list.size) {
            throw LoginsStorageException("MemoryLoginsStorage was provided with logins list that had duplicated IDs")
        }
    }

    override fun close() {
        synchronized(this) {
            state = LoginsStorageState.Closed
        }
    }

    override fun lock(): SyncResult<Unit> {
        return asyncResult {
            checkNotClosed()
            if (state == LoginsStorageState.Locked) {
                throw MismatchedLockException("Lock called when we are already locked")
            }
            state = LoginsStorageState.Locked
        }
    }

    override fun unlock(encryptionKey: String): SyncResult<Unit> {
        return asyncResult {
            checkNotClosed()
            if (state == LoginsStorageState.Unlocked) {
                throw MismatchedLockException("Unlock called when we are already unlocked")
            }
            state = LoginsStorageState.Unlocked
        }
    }

    override fun isLocked(): SyncResult<Boolean> {
        return asyncResult {
            state == LoginsStorageState.Locked
        }
    }

    override fun sync(syncInfo: SyncUnlockInfo): SyncResult<Unit> {
        return asyncResult {
            checkUnlocked()
            Log.w("MemoryLoginsStorage", "Not syncing because this implementation can not sync")
            Unit
        }
    }

    override fun reset(): SyncResult<Unit> {
        return asyncResult {
            checkUnlocked()
            Log.w("MemoryLoginsStorage", "Reset is a noop becasue this implementation can not sync")
            Unit
        }
    }

    override fun wipe(): SyncResult<Unit> {
        return asyncResult {
            checkUnlocked()
            list = ArrayList()
        }
    }

    override fun delete(id: String): SyncResult<Boolean> {
        return asyncResult {
            checkUnlocked()
            val oldLen = list.size
            list = list.filter { it.id != id }
            oldLen != list.size
        }
    }

    override fun get(id: String): SyncResult<ServerPassword?> {
        return asyncResult {
            checkUnlocked()
            list.find { it.id == id }
        }
    }

    override fun touch(id: String): SyncResult<Unit> {
        return asyncResult {
            checkUnlocked()
            val sp = list.find { it.id == id }
            if (sp != null) {
                // ServerPasswords are immutable, so we remove the current one from the list and
                // add a new one with updated properties
                list = list.filter { it.id != id };
                // Is there a better way to do this?
                val newsp = ServerPassword(
                        id = sp.id,
                        hostname = sp.hostname,
                        username = sp.username,
                        password = sp.password,
                        httpRealm = sp.httpRealm,
                        formSubmitURL = sp.formSubmitURL,
                        timeCreated = sp.timeCreated,
                        timePasswordChanged = sp.timePasswordChanged,
                        usernameField = sp.usernameField,
                        passwordField = sp.passwordField,
                        timeLastUsed = System.currentTimeMillis(),
                        timesUsed = sp.timesUsed + 1
                )
                list += newsp
            }
        }
    }

    override fun list(): SyncResult<List<ServerPassword>> {
        return asyncResult {
            checkUnlocked()
            // Return a copy so that mutations aren't visible (AIUI using `val` consistently in
            // ServerPassword means it's immutable, so it's fine that this is a shallow copy)
            ArrayList(list)
        }
    }

    private fun checkNotClosed() {
        if (state == LoginsStorageState.Closed) {
            throw LoginsStorageException("Using MemoryLoginsStorage after close!");
        }
    }

    private fun checkUnlocked() {
        if (state != LoginsStorageState.Unlocked) {
            throw LoginsStorageException("Using MemoryLoginsStorage without unlocking first: $state");
        }
    }

    private fun <T> asyncResult(callback: () -> T): SyncResult<T> {
        // Roughly mimic the semantics of MentatLoginsStorage -- serialize all calls to this API (
        // unlike MentatLoginsStorage we don't serialize calls to separate instances, but that
        // shouldn't matter that much).
        val result = SyncResult<T>()
        launch {
            synchronized(this@MemoryLoginsStorage) {
                try {
                    result.complete(callback());
                } catch (e: Exception) {
                    result.completeExceptionally(e);
                }
            }
        }
        return result;
    }

}
