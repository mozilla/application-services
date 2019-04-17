/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation
import UIKit

fileprivate let queue = DispatchQueue(label: "com.mozilla.logins-storage")

open class LoginsStorage {
    private var raw: UInt64 = 0
    let dbPath: String

    public init(databasePath: String) {
        self.dbPath = databasePath
    }

    deinit {
        self.close()
    }

    private func doDestroy() {
        let raw = self.raw
        self.raw = 0
        if raw != 0 {
            // Is `try!` the right thing to do? We should only hit an error here
            // for panics and handle misuse, both inidicate bugs in our code
            // (the first in the rust code, the 2nd in this swift wrapper).
            try! LoginsStoreError.unwrap({ err in
                sync15_passwords_state_destroy(raw, err)
            })
        }
    }

    /// Manually close the database (this is automatically called from deinit(), so
    /// manually calling it is usually unnecessary).
    open func close() {
        queue.sync(execute: {
            self.doDestroy()
        })
    }

    /// Test if the database is locked.
    open func isLocked() -> Bool {
        return queue.sync(execute: {
            return self.raw == 0
        })
    }

    // helper to reduce boilerplate, we don't use queue.sync
    // since we expect the caller to do so.
    private func getUnlocked() throws -> UInt64 {
        if self.raw == 0 {
            throw LockError.mismatched
        }
        return self.raw
    }

    /// Unlock the database.
    ///
    /// Throws `LockError.mismatched` if the database is already unlocked.
    ///
    /// Throws a `LoginStoreError.InvalidKey` if the key is incorrect, or if dbPath does not point
    /// to a database, (may also throw `LoginStoreError.Unspecified` or `.Panic`).
    open func unlock(withEncryptionKey key: String) throws {
        try queue.sync(execute: {
            if self.raw != 0 {
                throw LockError.mismatched
            }
            self.raw = try LoginsStoreError.unwrap({ err in
                sync15_passwords_state_new(self.dbPath, key, err)
            })
        })
    }

    /// equivalent to `unlock(withEncryptionKey:)`, but does not throw if the
    /// database is already unlocked.
    open func ensureUnlocked(withEncryptionKey key: String) throws {
        try queue.sync(execute: {
            self.raw = try LoginsStoreError.unwrap({ err in
                sync15_passwords_state_new(self.dbPath, key, err)
            })
        })
    }

    /// Lock the database.
    ///
    /// Throws `LockError.mismatched` if the database is already locked.
    open func lock() throws {
        try queue.sync(execute: {
            if self.raw == 0 {
                throw LockError.mismatched
            }
            self.doDestroy()
        })
    }

    /// Locks the database, but does not throw in the case that the database is
    /// already locked. This is an alias for `close()`, provided for convenience
    /// (and consistency with Android)
    open func ensureLocked() {
        close()
    }

    /// Synchronize with the server.
    open func sync(unlockInfo: SyncUnlockInfo) throws {
        try queue.sync(execute: {
            let engine = try self.getUnlocked()
            try LoginsStoreError.unwrap({ err in
                sync15_passwords_sync(engine, unlockInfo.kid, unlockInfo.fxaAccessToken, unlockInfo.syncKey, unlockInfo.tokenserverURL, err)
            })
        })
    }

    /// Delete all locally stored login sync metadata. It's unclear if
    /// there's ever a reason for users to call this
    open func reset() throws {
        try queue.sync(execute: {
            let engine = try self.getUnlocked()
            try LoginsStoreError.unwrap({ err in
                sync15_passwords_reset(engine, err)
            })
        })
    }

    /// Disable memory security, which prevents keys from being swapped to disk.
    /// This allows some esoteric attacks, but can have a performance benefit.
    open func disableMemSecurity() throws {
        try queue.sync(execute: {
            let engine = try self.getUnlocked()
            try LoginsStoreError.unwrap({ err in
                sync15_passwords_disable_mem_security(engine, err)
            })
        })
    }

    /// Delete all locally stored login data.
    open func wipe() throws {
        try queue.sync(execute: {
            let engine = try self.getUnlocked()
            try LoginsStoreError.unwrap({ err in
                sync15_passwords_wipe(engine, err)
            })
        })
    }

    open func wipeLocal() throws {
        try queue.sync(execute: {
            let engine = try self.getUnlocked()
            try LoginsStoreError.unwrap({ err in
                sync15_passwords_wipe_local(engine, err)
            })
        })
    }

    /// Delete the record with the given ID. Returns false if no such record existed.
    open func delete(id: String) throws -> Bool {
        return try queue.sync(execute: {
            let engine = try self.getUnlocked()
            let boolAsU8 = try LoginsStoreError.unwrap({ err in
                sync15_passwords_delete(engine, id, err)
            })
            return boolAsU8 != 0
        })
    }

    /// Bump the usage count for the record with the given id.
    ///
    /// Throws `LoginStoreError.NoSuchRecord` if there was no such record.
    open func touch(id: String) throws {
        try queue.sync(execute: {
            let engine = try self.getUnlocked()
            try LoginsStoreError.unwrap({ err in
                sync15_passwords_touch(engine, id, err)
            })
        })
    }

    /// Insert `login` into the database. If `login.id` is not empty,
    /// then this throws `LoginStoreError.DuplicateGuid` if there is a collision
    ///
    /// Returns the `id` of the newly inserted record.
    open func add(login: LoginRecord) throws -> String {
        let json = try login.toJSON()
        return try queue.sync(execute: {
            let engine = try self.getUnlocked()
            let ptr = try LoginsStoreError.unwrap({ err in
                sync15_passwords_add(engine, json, err)
            })
            return String(freeingRustString: ptr)
        })
    }

    /// Update `login` in the database. If `login.id` does not refer to a known
    /// login, then this throws `LoginStoreError.NoSuchRecord`.
    open func update(login: LoginRecord) throws {
        let json = try login.toJSON()
        return try queue.sync(execute: {
            let engine = try self.getUnlocked()
            return try LoginsStoreError.unwrap({ err in
                sync15_passwords_update(engine, json, err)
            })
        })
    }

    /// Get the record with the given id. Returns nil if there is no such record.
    open func get(id: String) throws -> LoginRecord? {
        return try queue.sync(execute: {
            let engine = try self.getUnlocked()
            let ptr = try LoginsStoreError.tryUnwrap({ err in
                sync15_passwords_get_by_id(engine, id, err)
            })
            guard let rustStr = ptr else {
                return nil
            }
            let jsonStr = String(freeingRustString: rustStr)
            return try LoginRecord(fromJSONString: jsonStr)
        })
    }

    /// Get the entire list of records.
    open func list() throws -> [LoginRecord] {
        return try queue.sync(execute: {
            let engine = try self.getUnlocked()
            let rustStr = try LoginsStoreError.unwrap({ err in
                sync15_passwords_get_all(engine, err)
            })
            let jsonStr = String(freeingRustString: rustStr)
            return try LoginRecord.fromJSONArray(jsonStr)
        })
    }

}

