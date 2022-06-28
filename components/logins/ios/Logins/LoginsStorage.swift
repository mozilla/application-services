/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation
import Glean
import UIKit

typealias LoginsStoreError = LoginsStorageError

open class LoginsStorage {
    private var store: LoginStore

    public init(databasePath: String) throws {
        store = try LoginStore(path: databasePath)
    }

    /// Delete all locally stored login sync metadata. It's unclear if
    /// there's ever a reason for users to call this
    open func reset() throws {
        try store.reset()
    }

    /// Delete all locally stored login data.
    open func wipe() throws {
        try store.wipe()
    }

    open func wipeLocal() throws {
        try store.wipeLocal()
    }

    /// Delete the record with the given ID. Returns false if no such record existed.
    open func delete(id: String) throws -> Bool {
        return try store.delete(id: id)
    }

    /// Bump the usage count for the record with the given id.
    ///
    /// Throws `LoginStoreError.NoSuchRecord` if there was no such record.
    open func touch(id: String) throws {
        try store.touch(id: id)
    }

    /// Insert `login` into the database. If `login.id` is not empty,
    /// then this throws `LoginStoreError.DuplicateGuid` if there is a collision
    ///
    /// Returns the `id` of the newly inserted record.
    open func add(login: LoginEntry, encryptionKey: String) throws -> EncryptedLogin {
        return try store.add(login: login, encryptionKey: encryptionKey)
    }

    /// Update `login` in the database. If `login.id` does not refer to a known
    /// login, then this throws `LoginStoreError.NoSuchRecord`.
    open func update(id: String, login: LoginEntry, encryptionKey: String) throws -> EncryptedLogin {
        return try store.update(id: id, login: login, encryptionKey: encryptionKey)
    }

    /// Get the record with the given id. Returns nil if there is no such record.
    open func get(id: String) throws -> EncryptedLogin? {
        return try store.get(id: id)
    }

    /// Get the entire list of records.
    open func list() throws -> [EncryptedLogin] {
        return try store.list()
    }

    /// Get the list of records for some base domain.
    open func getByBaseDomain(baseDomain: String) throws -> [EncryptedLogin] {
        return try store.getByBaseDomain(baseDomain: baseDomain)
    }

    /// Register with the sync manager
    open func registerWithSyncManager() throws {
        return store.registerWithSyncManager()
    }

    open func sync(unlockInfo: SyncUnlockInfo) throws -> String {
        return try store
            .sync(
                keyId: unlockInfo.kid,
                accessToken: unlockInfo.fxaAccessToken,
                syncKey: unlockInfo.syncKey,
                tokenserverUrl: unlockInfo.tokenserverURL,
                localEncryptionKey: unlockInfo.loginEncryptionKey
            )
    }
}

public func migrateLoginsWithMetrics(
    path: String,
    newEncryptionKey: String,
    sqlcipherPath: String,
    sqlcipherKey: String,
    salt: String
) -> Bool {
    var didMigrationSucceed = false

    do {
        try migrateLogins(
            path: path,
            newEncryptionKey: newEncryptionKey,
            sqlcipherPath: sqlcipherPath,
            sqlcipherKey: sqlcipherKey,
            salt: salt
        )
        didMigrationSucceed = true
    } catch let err as NSError {
        GleanMetrics.LoginsStoreMigration.errors.add(err.localizedDescription)
    }
    return didMigrationSucceed
}
