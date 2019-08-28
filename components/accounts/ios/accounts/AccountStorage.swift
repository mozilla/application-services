/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation

protocol AccountStorage {
    func read() -> String?
    func write(accountState: String)
    func clear()
}

// TODO: need to implement it with keychain!
class UnsafeUserDefaultStorage: AccountStorage {
    static let accountKey: String = "accounts.storage"
    func read() -> String? {
        return UserDefaults.standard.string(forKey: UnsafeUserDefaultStorage.accountKey)
    }

    func write(accountState: String) {
        UserDefaults.standard.set(accountState, forKey: UnsafeUserDefaultStorage.accountKey)
    }

    func clear() {
        UserDefaults.standard.removeObject(forKey: UnsafeUserDefaultStorage.accountKey)
    }
}
