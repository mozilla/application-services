/* Copyright 2018 Mozilla
 * Licensed under the Apache License, Version 2.0 (the "License"); you may not use
 * this file except in compliance with the License. You may obtain a copy of the
 * License at http://www.apache.org/licenses/LICENSE-2.0
 * Unless required by applicable law or agreed to in writing, software distributed
 * under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
 * CONDITIONS OF ANY KIND, either express or implied. See the License for the
 * specific language governing permissions and limitations under the License. */
package mozilla.appservices.logins

import android.util.Log
import kotlinx.coroutines.experimental.launch
import java.io.Closeable
import java.util.UUID

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
                    ?: throw NoSuchRecordException("No such record: $id")
            // ServerPasswords are immutable, so we remove the current one from the list and
            // add a new one with updated properties
            list = list.filter { it.id != id }

            val newsp = sp.copy(
                timeLastUsed = System.currentTimeMillis(),
                timesUsed = sp.timesUsed + 1
            )
            list += newsp
        }
    }

    override fun add(login: ServerPassword): SyncResult<String> {
        return asyncResult {
            checkUnlocked()

            val toInsert = if (login.id.isEmpty()) {
                // This isn't anything like what the IDs we generate in rust look like
                // but whatever.
                login.copy(id = UUID.randomUUID().toString())
            } else {
                login
            }.copy(
                timesUsed = 1,
                timeLastUsed = System.currentTimeMillis(),
                timeCreated = System.currentTimeMillis(),
                timePasswordChanged = System.currentTimeMillis()
            )

            checkValid(toInsert)

            val sp = list.find { it.id == toInsert.id }
            if (sp != null) {
                // Note: Not the way this is formatted in rust -- don't rely on the formatting!
                throw IdCollisionException("Id already exists " + toInsert.id)
            }

            list += toInsert
            toInsert.id
        }
    }

    override fun update(login: ServerPassword): SyncResult<Unit> {
        return asyncResult {
            checkUnlocked()
            val current = list.find { it.id == login.id }
                    ?: throw NoSuchRecordException("No such record: " + login.id)

            val newRecord = login.copy(
                    timeLastUsed = System.currentTimeMillis(),
                    timesUsed = current.timesUsed + 1,
                    timeCreated = current.timeCreated,
                    timePasswordChanged = if (current.password == login.password) {
                        current.timePasswordChanged
                    } else {
                        System.currentTimeMillis()
                    })

            checkValid(newRecord)

            list = list.filter { it.id != login.id }

            list += newRecord
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

    private fun checkValid(login: ServerPassword) {
        if (login.hostname == "") {
            throw InvalidRecordException("Invalid login: Hostname is empty")
        }
        if (login.password == "") {
            throw InvalidRecordException("Invalid login: Password is empty")
        }
        if (login.formSubmitURL != null && login.httpRealm != null) {
            throw InvalidRecordException(
                    "Invalid login: Both `formSubmitUrl` and `httpRealm` are present")
        }
        if (login.formSubmitURL == null && login.httpRealm == null) {
            throw InvalidRecordException(
                    "Invalid login: Neither `formSubmitUrl` and `httpRealm` are present")
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
