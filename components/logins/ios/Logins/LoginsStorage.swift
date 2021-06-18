/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation
import UIKit
#if canImport(Sync15)
    import Sync15
#endif

typealias LoginsStoreError = LoginsStorageError

/*
 ** We probably should have this class go away eventually as it's really only a thin wrapper
 * similar to its kotlin equiavlent, however the only thing preventing this from being removed is
 * the queue.sync which we should be moved over to the consumer side of things
 */
open class LoginsStorage {
    private var store: LoginStore?
    let dbPath: String
    private let queue = DispatchQueue(label: "com.mozilla.logins-storage")

    public init(databasePath: String) {
        dbPath = databasePath
    }

    deinit {
        self.close()
    }

    private func doDestroy() {
        store = nil
    }

    /// Manually close the database (this is automatically called from deinit(), so
    /// manually calling it is usually unnecessary).
    open func close() {
        queue.sync {
            self.doDestroy()
        }
    }

    /// Test if the database is locked.
    open func isLocked() -> Bool {
        return queue.sync {
            self.store == nil
        }
    }

    // helper to reduce boilerplate, we don't use queue.sync
    // since we expect the caller to do so.
    private func getUnlockedStore() throws -> LoginStore {
        if store == nil {
            throw LoginsStoreError.MismatchedLock(message: "Mismatched Lock")
        }
        return store!
    }

    /// Unlock the database and reads the salt.
    ///
    /// Throws `LockError.mismatched` if the database is already unlocked.
    ///
    /// Throws a `LoginStoreError.InvalidKey` if the key is incorrect, or if dbPath does not point
    /// to a database, (may also throw `LoginStoreError.Unspecified` or `.Panic`).
    open func getDbSaltForKey(key: String) throws -> String {
        try queue.sync {
            if self.store != nil {
                throw LoginsStoreError.MismatchedLock(message: "Mismatched Lock")
            }
            return try openAndGetSalt(path: self.dbPath, encryptionKey: key)
        }
    }

    /// Migrate an existing database to a sqlcipher plaintext header.
    /// If your application calls this method without reading and persisting
    /// the salt, the database will be rendered un-usable.
    ///
    /// Throws `LockError.mismatched` if the database is already unlocked.
    ///
    /// Throws a `LoginStoreError.InvalidKey` if the key is incorrect, or if dbPath does not point
    /// to a database, (may also throw `LoginStoreError.Unspecified` or `.Panic`).
    open func migrateToPlaintextHeader(key: String, salt: String) throws {
        try queue.sync {
            if self.store != nil {
                throw LoginsStoreError.MismatchedLock(message: "Mismatched Lock")
            }
            try openAndMigrateToPlaintextHeader(path: self.dbPath, encryptionKey: key, salt: salt)
        }
    }

    private func doOpen(_ key: String, salt: String?) throws {
        if store != nil {
            return
        }
        if let salt = salt {
            store = try LoginStore.newWithSalt(path: dbPath, encryptionKey: key, salt: salt)
        } else {
            store = try LoginStore(path: dbPath, encryptionKey: key)
        }
    }

    /// Unlock the database.
    /// `key` must be a random string.
    /// `salt` must be an hex-encoded string of 32 characters (e.g. `a6a97a03ac3e5a20617175355ea2da5c`).
    ///
    /// Throws `LockError.mismatched` if the database is already unlocked.
    ///
    /// Throws a `LoginStoreError.InvalidKey` if the key is incorrect, or if dbPath does not point
    /// to a database, (may also throw `LoginStoreError.Unspecified` or `.Panic`).
    open func unlockWithKeyAndSalt(key: String, salt: String) throws {
        try queue.sync {
            if self.store != nil {
                throw LoginsStoreError.MismatchedLock(message: "Mismatched Lock")
            }
            try self.doOpen(key, salt: salt)
        }
    }

    /// Equivalent to `unlockWithKeyAndSalt(key:, salt:)`, but does not throw if the
    /// database is already unlocked.
    open func ensureUnlockedWithKeyAndSalt(key: String, salt: String) throws {
        try queue.sync {
            try self.doOpen(key, salt: salt)
        }
    }

    /// Unlock the database.
    ///
    /// Throws `LockError.mismatched` if the database is already unlocked.
    ///
    /// Throws a `LoginStoreError.InvalidKey` if the key is incorrect, or if dbPath does not point
    /// to a database, (may also throw `LoginStoreError.Unspecified` or `.Panic`).
    @available(*, deprecated, message: "Use unlockWithKeyAndSalt instead.")
    open func unlock(withEncryptionKey key: String) throws {
        try queue.sync {
            if self.store != nil {
                throw LoginsStoreError.MismatchedLock(message: "Mismatched Lock")
            }
            try self.doOpen(key, salt: nil)
        }
    }

    /// equivalent to `unlock(withEncryptionKey:)`, but does not throw if the
    /// database is already unlocked.
    @available(*, deprecated, message: "Use ensureUnlockedWithKeyAndSalt instead.")
    open func ensureUnlocked(withEncryptionKey key: String) throws {
        try queue.sync {
            try self.doOpen(key, salt: nil)
        }
    }

    /// Lock the database.
    ///
    /// Throws `LockError.mismatched` if the database is already locked.
    open func lock() throws {
        try queue.sync {
            if self.store == nil {
                throw LoginsStoreError.MismatchedLock(message: "Mismatched Lock")
            }
            self.doDestroy()
        }
    }

    /// Locks the database, but does not throw in the case that the database is
    /// already locked. This is an alias for `close()`, provided for convenience
    /// (and consistency with Android)
    open func ensureLocked() {
        close()
    }

    /// Delete all locally stored login sync metadata. It's unclear if
    /// there's ever a reason for users to call this
    open func reset() throws {
        try queue.sync {
            try getUnlockedStore().reset()
        }
    }

    /// Disable memory security, which prevents keys from being swapped to disk.
    /// This allows some esoteric attacks, but can have a performance benefit.
    open func disableMemSecurity() throws {
        try queue.sync {
            try getUnlockedStore().disableMemSecurity()
        }
    }

    open func rekeyDatabase(withNewEncryptionKey newKey: String) throws {
        try queue.sync {
            try self.getUnlockedStore().rekeyDatabase(newEncryptionKey: newKey)
        }
    }

    /// Delete all locally stored login data.
    open func wipe() throws {
        try queue.sync {
            try self.getUnlockedStore().wipe()
        }
    }

    open func wipeLocal() throws {
        try queue.sync {
            try self.getUnlockedStore().wipeLocal()
        }
    }

    /// Delete the record with the given ID. Returns false if no such record existed.
    open func delete(id: String) throws -> Bool {
        return try queue.sync {
            return try self.getUnlockedStore().delete(id: id)
        }
    }

    /// Ensure that the record is valid and a duplicate record doesn't exist.
    open func ensureValid(login: Login) throws {
        try queue.sync {
            try self.getUnlockedStore().checkValidWithNoDupes(login: login)
        }
    }

    /// Bump the usage count for the record with the given id.
    ///
    /// Throws `LoginStoreError.NoSuchRecord` if there was no such record.
    open func touch(id: String) throws {
        try queue.sync {
            try self.getUnlockedStore().touch(id: id)
        }
    }

    /// Insert `login` into the database. If `login.id` is not empty,
    /// then this throws `LoginStoreError.DuplicateGuid` if there is a collision
    ///
    /// Returns the `id` of the newly inserted record.
    open func add(login: Login) throws -> String {
        return try queue.sync {
            return try self.getUnlockedStore().add(login: login)
        }
    }

    /// Update `login` in the database. If `login.id` does not refer to a known
    /// login, then this throws `LoginStoreError.NoSuchRecord`.
    open func update(login: Login) throws {
        try queue.sync {
            try self.getUnlockedStore().update(login: login)
        }
    }

    /// Get the record with the given id. Returns nil if there is no such record.
    open func get(id: String) throws -> Login? {
        return try queue.sync {
            return try self.getUnlockedStore().get(id: id)
        }
    }

    /// Get the entire list of records.
    open func list() throws -> [Login] {
        return try queue.sync {
            return try self.getUnlockedStore().list()
        }
    }

    /// Get the set of potential duplicates ignoring the username of `login`.
    open func potentialDupesIgnoringUsername(to login: Login) throws -> [Login] {
        return try queue.sync {
            return try self.getUnlockedStore().potentialDupesIgnoringUsername(login: login)
        }
    }

    /// Get the list of records for some base domain.
    open func getByBaseDomain(baseDomain: String) throws -> [Login] {
        return try queue.sync {
            return try self.getUnlockedStore().getByBaseDomain(baseDomain: baseDomain)
        }
    }

    /// Register with the sync manager
    open func registerWithSyncManager() throws {
        return try queue.sync {
            return try self.getUnlockedStore().registerWithSyncManager()
        }
    }

    open func sync(unlockInfo: SyncUnlockInfo) throws -> String {
        return try queue.sync {
            return try self.getUnlockedStore()
                .sync(
                    keyId: unlockInfo.kid,
                    accessToken: unlockInfo.fxaAccessToken,
                    syncKey: unlockInfo.syncKey,
                    tokenserverUrl: unlockInfo.tokenserverURL
                )
        }
    }
}
