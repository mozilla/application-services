/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation

open class TabsStorage {
    private var store: TabsStore

    public init(databasePath: String) {
        store = TabsStore(path: databasePath)
    }

    /// Get all tabs by client.
    open func getAll() async -> [ClientRemoteTabs] {
        Task {
            self.store.getAll()
        }
    }

    /// Set the local tabs.
    open func setLocalTabs(remoteTabs: [RemoteTabRecord]) async {
        Task {
            self.store.setLocalTabs(remoteTabs: remoteTabs)
        }
    }

    open func registerWithSyncManager() async {
        Task {
            self.store.registerWithSyncManager()
        }
    }
}
