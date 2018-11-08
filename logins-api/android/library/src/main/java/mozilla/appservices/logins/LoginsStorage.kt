/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.logins

class SyncUnlockInfo (
        val kid: String,
        val fxaAccessToken: String,
        val syncKey: String,
        val tokenserverURL: String
)

interface LoginsStorage : AutoCloseable {

    fun lock()

    fun unlock(encryptionKey: String)

    fun isLocked(): Boolean

    /**
     * Synchronize the logins storage layer with a remote layer.
     */
    fun sync(syncInfo: SyncUnlockInfo)

    /**
     * Delete all locally stored login sync metadata.
     */
    fun reset()

    /**
     * Delete all locally stored login data.
     */
    fun wipe()

    /**
     * Delete a password with the given ID.
     */
    fun delete(id: String): Boolean

    /**
     * Fetch a password from the underlying storage layer by ID.
     */
    fun get(id: String): ServerPassword?

    /**
     * Mark the login with the given ID as `in-use`.
     */
    fun touch(id: String)

    /**
     * Fetch the full list of passwords from the underlying storage layer.
     */
    fun list(): List<ServerPassword>

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
    fun add(login: ServerPassword): String

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
    fun update(login: ServerPassword)

}
