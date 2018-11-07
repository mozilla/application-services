/* Copyright 2018 Mozilla
 * Licensed under the Apache License, Version 2.0 (the "License"); you may not use
 * this file except in compliance with the License. You may obtain a copy of the
 * License at http://www.apache.org/licenses/LICENSE-2.0
 * Unless required by applicable law or agreed to in writing, software distributed
 * under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
 * CONDITIONS OF ANY KIND, either express or implied. See the License for the
 * specific language governing permissions and limitations under the License. */
package mozilla.appservices.logins

import java.io.Closeable

class SyncUnlockInfo (
        val kid: String,
        val fxaAccessToken: String,
        val syncKey: String,
        val tokenserverURL: String
)

interface LoginsStorage : Closeable {

    fun lock(): SyncResult<Unit>

    fun unlock(encryptionKey: String): SyncResult<Unit>

    fun isLocked(): SyncResult<Boolean>

    /**
     * Synchronize the logins storage layer with a remote layer.
     */
    fun sync(syncInfo: SyncUnlockInfo): SyncResult<Unit>

    /**
     * Delete all locally stored login sync metadata.
     */
    fun reset(): SyncResult<Unit>

    /**
     * Delete all locally stored login data.
     */
    fun wipe(): SyncResult<Unit>

    /**
     * Delete a password with the given ID.
     */
    fun delete(id: String): SyncResult<Boolean>

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

    /**
     * Insert the provided login into the database.
     *
     * This function ignores values in metadata fields (`timesUsed`,
     * `timeCreated`, `timeLastUsed`, and `timePasswordChanged`).
     *
     * If login has an empty id field, then a GUID will be
     * generated automatically. The format of generated guids
     * are left up to the implementation of LoginsStorage (in
     * practice the [DatabaseLoginsStorage] generates 12-character
     * base64url (RFC 4648) encoded strings, and [MemoryLoginsStorage]
     * generates strings using [java.util.UUID.toString])
     *
     * This will return an error result if a GUID is provided but
     * collides with an existing record, or if the provided record
     * is invalid (missing password, hostname, or doesn't have exactly
     * one of formSubmitURL and httpRealm).
     */
    fun add(login: ServerPassword): SyncResult<String>

    /**
     * Update the fields in the provided record.
     *
     * This will return an error if `login.id` does not refer to
     * a record that exists in the database, or if the provided record
     * is invalid (missing password, hostname, or doesn't have exactly
     * one of formSubmitURL and httpRealm).
     *
     * Like `add`, this function will ignore values in metadata
     * fields (`timesUsed`, `timeCreated`, `timeLastUsed`, and
     * `timePasswordChanged`).
     */
    fun update(login: ServerPassword): SyncResult<Unit>

}
