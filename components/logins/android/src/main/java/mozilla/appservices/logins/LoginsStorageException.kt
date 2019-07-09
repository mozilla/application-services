/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.logins

// TODO: More descriptive errors would be nice here...
open class LoginsStorageException(msg: String) : Exception(msg)

/** This indicates that the sync authentication is invalid, likely due to having
 * expired.
 */
class SyncAuthInvalidException(msg: String) : LoginsStorageException(msg)

/**
 * This is thrown if `lock()`/`unlock()` pairs don't match up.
 */
class MismatchedLockException(msg: String) : LoginsStorageException(msg)

/**
 * This is thrown if `update()` is performed with a record whose ID
 * does not exist.
 */
class NoSuchRecordException(msg: String) : LoginsStorageException(msg)

/**
 * This is thrown if `add()` is given a record that has an ID, and
 * that ID does not exist.
 */
class IdCollisionException(msg: String) : LoginsStorageException(msg)

/**
 * This is thrown on attempts to insert or update a record so that it
 * is no longer valid. Valid records have:
 *
 * - non-empty hostnames
 * - non-empty passwords
 * - and exactly one of `httpRealm` or `formSubmitUrl` is non-null.
 */
class InvalidRecordException(msg: String) : LoginsStorageException(msg)

/**
 * This error is emitted in two cases:
 *
 * 1. An incorrect key is used to to open the login database
 * 2. The file at the path specified is not a sqlite database.
 */
class InvalidKeyException(msg: String) : LoginsStorageException(msg)

/**
 * This error is emitted if a request to a sync server failed.
 */
class RequestFailedException(msg: String) : LoginsStorageException(msg)

/**
 * This error is emitted if a sync or other operation is interrupted.
 */
class InterruptedException(msg: String) : LoginsStorageException(msg)
