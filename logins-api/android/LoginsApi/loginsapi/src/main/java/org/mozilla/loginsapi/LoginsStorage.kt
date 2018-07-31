package org.mozilla.loginsapi

import java.io.Closeable

class SyncUnlockInfo (
        val kid: String,
        val fxaAccessToken: String,
        val syncKey: String,
        val tokenserverBaseURL: String
)

interface LoginsStorage : Closeable {

    fun lock(): SyncResult<Unit>

    fun unlock(encryptionKey: String, syncInfo: SyncUnlockInfo): SyncResult<Unit>

    fun isLocked(): SyncResult<Boolean>

    /**
     * Synchronize the logins storage layer with a remote layer.
     */
    fun sync(): SyncResult<Unit>

    /**
     * Delete all locally stored login sync metadata.
     */
    fun reset(): SyncResult<Unit>

    /**
     * Delete all locally stored login data.
     */
    fun wipe(): SyncResult<Unit>

    /**
     * Delete a password with the given ID. TODO: should be SyncResult<bool>!
     */
    fun delete(id: String): SyncResult<Unit>

    /**
     * Fetch a password from the underlying storage layer by ID.
     */
    fun get(id: String): SyncResult<ServerPassword?>

    /**
     * Mark the login with the given ID as `in-use`.
     */
    fun touch(id: String): SyncResult<Unit>

    /**
     * Fetch the full list of passwords from the underlying storage layer.
     */
    fun list(): SyncResult<List<ServerPassword>>
}
