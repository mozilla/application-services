/* Copyright 2018 Mozilla
 * Licensed under the Apache License, Version 2.0 (the "License"); you may not use
 * this file except in compliance with the License. You may obtain a copy of the
 * License at http://www.apache.org/licenses/LICENSE-2.0
 * Unless required by applicable law or agreed to in writing, software distributed
 * under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
 * CONDITIONS OF ANY KIND, either express or implied. See the License for the
 * specific language governing permissions and limitations under the License. */
package org.mozilla.sync15.logins

// TODO: More descriptive errors would be nice here...
open class LoginsStorageException(msg: String): Exception(msg)

/** This indicates that the sync authentication is invalid, likely due to having
 * expired.
 */
class SyncAuthInvalidException(msg: String): LoginsStorageException(msg)

/**
 * This is thrown if `lock()`/`unlock()` pairs don't match up.
 */
class MismatchedLockException(msg: String): LoginsStorageException(msg)

/**
 * This is thrown if `update()` is performed with a record whose ID
 * does not exist.
 */
class NoSuchRecordException(msg: String): LoginsStorageException(msg)

/**
 * This is thrown if `add()` is given a record that has an ID, and
 * that ID does not exist.
 */
class IdCollisionException(msg: String): LoginsStorageException(msg)

/**
 * This is thrown on attempts to insert or update a record so that it
 * is no longer valid. Valid records have:
 *
 * - non-empty hostnames
 * - non-empty passwords
 * - and exactly one of `httpRealm` or `formSubmitUrl` is non-null.
 */
class InvalidRecordException(msg: String): LoginsStorageException(msg)

/**
 * This error is emitted in two cases:
 *
 * 1. An incorrect key is used to to open the login database
 * 2. The file at the path specified is not a sqlite database.
 */
class InvalidKeyException(msg: String): LoginsStorageException(msg)

/**
 * This error is emitted if a request to a sync server failed.
 */
class RequestFailedException(msg: String): LoginsStorageException(msg)


