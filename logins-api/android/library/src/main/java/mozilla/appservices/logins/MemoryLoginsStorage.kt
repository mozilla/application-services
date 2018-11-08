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
import java.util.UUID

private enum class LoginsStorageState {
    Unlocked,
    Locked,
    Closed,
}

class MemoryLoginsStorage(private var list: List<ServerPassword>) : AutoCloseable, LoginsStorage {
    private var state: LoginsStorageState = LoginsStorageState.Locked;

    init {
        // Check that the list we were given as input doesn't have any duplicated IDs.
        val ids = HashSet<String>(list.map { it.id })
        if (ids.size != list.size) {
            throw LoginsStorageException("MemoryLoginsStorage was provided with logins list that had duplicated IDs")
        }
    }

    @Synchronized
    override fun close() {
        state = LoginsStorageState.Closed
    }

    @Synchronized
    override fun lock() {
        checkNotClosed()
        if (state == LoginsStorageState.Locked) {
            throw MismatchedLockException("Lock called when we are already locked")
        }
        state = LoginsStorageState.Locked
    }

    @Synchronized
    override fun unlock(encryptionKey: String) {
        checkNotClosed()
        if (state == LoginsStorageState.Unlocked) {
            throw MismatchedLockException("Unlock called when we are already unlocked")
        }
        state = LoginsStorageState.Unlocked
    }


    @Synchronized
    override fun isLocked(): Boolean {
        return state == LoginsStorageState.Locked
    }

    @Synchronized
    override fun sync(syncInfo: SyncUnlockInfo) {
        checkUnlocked()
        Log.w("MemoryLoginsStorage", "Not syncing because this implementation can not sync")
    }

    @Synchronized
    override fun reset() {
        checkUnlocked()
        Log.w("MemoryLoginsStorage", "Reset is a noop becasue this implementation can not sync")
    }

    @Synchronized
    override fun wipe() {
        checkUnlocked()
        list = ArrayList()
    }

    @Synchronized
    override fun delete(id: String): Boolean {
        checkUnlocked()
        val oldLen = list.size
        list = list.filter { it.id != id }
        return oldLen != list.size
    }

    @Synchronized
    override fun get(id: String): ServerPassword? {
        checkUnlocked()
        return list.find { it.id == id }
    }

    @Synchronized
    override fun touch(id: String) {
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

    @Synchronized
    override fun add(login: ServerPassword): String {
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
        return toInsert.id
    }

    @Synchronized
    override fun update(login: ServerPassword) {
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

    @Synchronized
    override fun list(): List<ServerPassword> {
        checkUnlocked()
        // Return a copy so that mutations aren't visible (AIUI using `val` consistently in
        // ServerPassword means it's immutable, so it's fine that this is a shallow copy)
        return ArrayList(list)
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

}
