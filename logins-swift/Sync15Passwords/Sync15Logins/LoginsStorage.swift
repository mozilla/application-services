/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation
import UIKit

/// Set of arguments requires to sync.
open class SyncUnlockInfo {
    public var kid: String
    public var fxaAccessToken: String
    public var syncKey: String
    public var tokenserverURL: String

    public init (kid: String, fxaAccessToken: String, syncKey: String, tokenserverURL: String) {
        self.kid = kid
        self.fxaAccessToken = fxaAccessToken
        self.syncKey = syncKey
        self.tokenserverURL = tokenserverURL
    }
}

// We use a serial queue to protect access to the rust object.
let queue = DispatchQueue(label: "com.sync15.logins")

open class LoginsStorage {
    var raw: OpaquePointer? = nil
    let dbPath: String
    
    public init(databasePath: String) {
        self.dbPath = databasePath
    }
    
    deinit {
        queue.sync(execute: {
            doDestroy()
        })
    }
    
    private func doDestroy() {
        if let raw = self.raw {
            self.raw = nil
            sync15_passwords_state_destroy(raw)
        }
    }

    open func close() {
        self.doDestroy()
    }

    /// Test if the database is locked.
    open func isLocked() -> Bool {
        return queue.sync(execute: {
            return self.raw == nil
        })
    }

    /// Unlock the database.
    ///
    /// Throws `LockError.mismatched` if the database is already unlocked.
    ///
    /// Throws a `LoginStoreError.InvalidKey` if the key is incorrect, or if dbPath does not point
    /// to a database, (may also throw `LoginStoreError.Unspecified` or `.Panic`).
    open func unlock(withEncryptionKey key: String) throws {
        try queue.sync(execute: {
            if self.raw != nil {
                throw LockError.mismatched
            }
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
            if self.raw == nil {
                throw LockError.mismatched
            }
            self.doDestroy()
        })
    }
    
    /// Synchronize with the server.
    open func sync(unlockInfo: SyncUnlockInfo) throws {
        try queue.sync(execute: {
            guard let engine = self.raw else {
                throw LockError.locked
            }
            try LoginsStoreError.unwrap({ err in
                sync15_passwords_sync(engine,
                                            unlockInfo.kid,
                                            unlockInfo.fxaAccessToken,
                                            unlockInfo.syncKey,
                                            unlockInfo.tokenserverURL,
                                            err)
            })
        })
    }

    /// Delete all locally stored login sync metadata. It's unclear if
    /// there's ever a reason for users to call this
    open func reset() throws {
        try queue.sync(execute: {
            guard let engine = self.raw else {
                throw LockError.locked
            }
            try LoginsStoreError.unwrap({ err in
                sync15_passwords_reset(engine, err)
            })
        })
    }

    /// Delete all locally stored login data.
    open func wipe() throws {
        try queue.sync(execute: {
            guard let engine = self.raw else {
                throw LockError.locked
            }
            try LoginsStoreError.unwrap({ err in
                sync15_passwords_wipe(engine, err)
            })
        })
    }
    
    /// Delete the record with the given ID. Returns false if no such record existed.
    open func delete(id: String) throws -> Bool {
        return try queue.sync(execute: {
            guard let engine = self.raw else {
                throw LockError.locked
            }
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
            guard let engine = self.raw else {
                throw LockError.locked
            }
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
            guard let engine = self.raw else {
                throw LockError.locked
            }
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
            guard let engine = self.raw else {
                throw LockError.locked
            }
            return try LoginsStoreError.unwrap({ err in
                sync15_passwords_update(engine, json, err)
            })
        })
    }

    /// Get the record with the given id. Returns nil if there is no such record.
    open func get(id: String) throws -> LoginRecord? {
        return try queue.sync(execute: {
            guard let engine = self.raw else {
                throw LockError.locked
            }
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
            guard let engine = self.raw else {
                throw LockError.locked
            }
            let rustStr = try LoginsStoreError.unwrap({ err in
                sync15_passwords_get_all(engine, err)
            })
            let jsonStr = String(freeingRustString: rustStr)
            return try LoginRecord.fromJSONArray(jsonStr)
        })
    }

}

