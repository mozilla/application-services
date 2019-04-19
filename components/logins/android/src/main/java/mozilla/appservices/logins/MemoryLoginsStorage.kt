/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.logins

import android.util.Log
import java.util.UUID

private enum class LoginsStorageState {
    Unlocked,
    Locked,
    Closed,
}

class MemoryLoginsStorage(private var list: List<ServerPassword>) : AutoCloseable, LoginsStorage {

    private var state: LoginsStorageState = LoginsStorageState.Locked

    init {
        // Check that the list we were given as input doesn't have any duplicated IDs.
        val ids = HashSet<String>(list.map { it.id })
        if (ids.size != list.size) {
            throw LoginsStorageException("MemoryLoginsStorage was provided with logins list that had duplicated IDs")
        }
    }

    @Synchronized
    @Throws(LoginsStorageException::class)
    override fun close() {
        state = LoginsStorageState.Closed
    }

    @Synchronized
    @Throws(LoginsStorageException::class)
    override fun lock() {
        checkNotClosed()
        if (state == LoginsStorageState.Locked) {
            throw MismatchedLockException("Lock called when we are already locked")
        }
        state = LoginsStorageState.Locked
    }

    @Synchronized
    @Throws(LoginsStorageException::class)
    override fun unlock(encryptionKey: String) {
        checkNotClosed()
        if (state == LoginsStorageState.Unlocked) {
            throw MismatchedLockException("Unlock called when we are already unlocked")
        }
        state = LoginsStorageState.Unlocked
    }

    @Synchronized
    @Throws(LoginsStorageException::class)
    override fun unlock(encryptionKey: ByteArray) {
        // Currently we never check the key for the in-memory version, so this is fine.
        unlock("")
    }

    @Synchronized
    override fun isLocked(): Boolean {
        return state == LoginsStorageState.Locked
    }

    @Synchronized
    @Throws(LoginsStorageException::class)
    override fun ensureUnlocked(encryptionKey: String) {
        if (isLocked()) {
            this.unlock(encryptionKey)
        }
    }

    @Synchronized
    @Throws(LoginsStorageException::class)
    override fun ensureUnlocked(encryptionKey: ByteArray) {
        if (isLocked()) {
            this.unlock(encryptionKey)
        }
    }

    @Synchronized
    override fun ensureLocked() {
        if (!isLocked()) {
            this.lock()
        }
    }

    @Synchronized
    @Throws(LoginsStorageException::class)
    override fun sync(syncInfo: SyncUnlockInfo) {
        checkUnlocked()
        Log.w("MemoryLoginsStorage", "Not syncing because this implementation can not sync")
    }

    @Synchronized
    @Throws(LoginsStorageException::class)
    override fun reset() {
        checkUnlocked()
        Log.w("MemoryLoginsStorage", "Reset is a noop becasue this implementation can not sync")
    }

    @Synchronized
    @Throws(LoginsStorageException::class)
    override fun wipe() {
        checkUnlocked()
        list = ArrayList()
    }

    @Synchronized
    @Throws(LoginsStorageException::class)
    override fun wipeLocal() {
        checkUnlocked()
        // No remote state.
        list = ArrayList()
    }

    @Synchronized
    @Throws(LoginsStorageException::class)
    override fun delete(id: String): Boolean {
        checkUnlocked()
        val oldLen = list.size
        list = list.filter { it.id != id }
        return oldLen != list.size
    }

    @Synchronized
    @Throws(LoginsStorageException::class)
    override fun get(id: String): ServerPassword? {
        checkUnlocked()
        return list.find { it.id == id }
    }

    @Synchronized
    @Throws(LoginsStorageException::class)
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
    @Throws(LoginsStorageException::class)
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
    @Throws(LoginsStorageException::class)
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
    @Throws(LoginsStorageException::class)
    override fun list(): List<ServerPassword> {
        checkUnlocked()
        // Return a copy so that mutations aren't visible (AIUI using `val` consistently in
        // ServerPassword means it's immutable, so it's fine that this is a shallow copy)
        return ArrayList(list)
    }

    private fun checkNotClosed() {
        if (state == LoginsStorageState.Closed) {
            throw LoginsStorageException("Using MemoryLoginsStorage after close!")
        }
    }

    private fun checkUnlocked() {
        if (state != LoginsStorageState.Unlocked) {
            throw LoginsStorageException("Using MemoryLoginsStorage without unlocking first: $state")
        }
    }

    @Suppress("ThrowsCount")
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
