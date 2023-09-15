/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation
import Glean
import UIKit

typealias LoginsStoreError = LoginsApiError

/*
 ** We probably should have this class go away eventually as it's really only a thin wrapper
 * similar to its kotlin equiavlent, however the only thing preventing this from being removed is
 * the queue.sync which we should be moved over to the consumer side of things
 */
open class LoginsStorage {
    private var store: LoginStore
    private let queue = DispatchQueue(label: "com.mozilla.logins-storage")

    public init(databasePath: String) throws {
        store = try LoginStore(path: databasePath)
    }

    open func wipeLocal() throws {
        try queue.sync {
            try self.store.wipeLocal()
        }
    }

    /// Delete the record with the given ID. Returns false if no such record existed.
    open func delete(id: String) throws -> Bool {
        return try queue.sync {
            try self.store.delete(id: id)
        }
    }

    /// Bump the usage count for the record with the given id.
    ///
    /// Throws `LoginStoreError.NoSuchRecord` if there was no such record.
    open func touch(id: String) throws {
        try queue.sync {
            try self.store.touch(id: id)
        }
    }

    /// Insert `login` into the database. If `login.id` is not empty,
    /// then this throws `LoginStoreError.DuplicateGuid` if there is a collision
    ///
    /// Returns the `id` of the newly inserted record.
    open func add(login: LoginEntry, encryptionKey: String) throws -> EncryptedLogin {
        return try queue.sync {
            try self.store.add(login: login, encryptionKey: encryptionKey)
        }
    }

    /// Update `login` in the database. If `login.id` does not refer to a known
    /// login, then this throws `LoginStoreError.NoSuchRecord`.
    open func update(id: String, login: LoginEntry, encryptionKey: String) throws -> EncryptedLogin {
        return try queue.sync {
            try self.store.update(id: id, login: login, encryptionKey: encryptionKey)
        }
    }

    /// Get the record with the given id. Returns nil if there is no such record.
    open func get(id: String) throws -> EncryptedLogin? {
        return try queue.sync {
            try self.store.get(id: id)
        }
    }

    /// Get the entire list of records.
    open func list() throws -> [EncryptedLogin] {
        return try queue.sync {
            try self.store.list()
        }
    }

    /// Get the list of records for some base domain.
    open func getByBaseDomain(baseDomain: String) throws -> [EncryptedLogin] {
        return try queue.sync {
            try self.store.getByBaseDomain(baseDomain: baseDomain)
        }
    }

    /// Register with the sync manager
    open func registerWithSyncManager() {
        return queue.sync {
            self.store.registerWithSyncManager()
        }
    }
}
