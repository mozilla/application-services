/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation
import UIKit

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

    /// Bump the usage count for the record with the given id.
    ///
    /// Throws `LoginStoreError.NoSuchRecord` if there was no such record.
    open func touch(id: String) throws {
        try queue.sync {
            try self.getUnlockedStore().touch(id: id)
        }
    }

    open func addOrUpdate(encKey: String, login: LoginFields) throws -> String {
        return try queue.sync {
            return try self.getUnlockedStore().addOrUpdate(encKey: encKey, login: login)
        }
    }

    open func decryptAndFixupLogin(encKey: String, login: Login) throws {
        try queue.sync {
            try self.getUnlockedStore().decryptAndFixupLogin(encKey: encKey, login: login)
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
