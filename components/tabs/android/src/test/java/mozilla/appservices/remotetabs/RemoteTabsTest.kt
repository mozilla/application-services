/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.remotetabs

import mozilla.appservices.syncmanager.SyncManager
import org.junit.Assert
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(manifest = Config.NONE)
class RemoteTabsTest {
    @Rule
    @JvmField
    val dbFolder = TemporaryFolder()

    private fun getTestStore(): TabsStore {
        return TabsStore(path = dbFolder.newFile().absolutePath)
    }

    @Test
    fun setLocalTabsTest() {
        val store = getTestStore()
        store.use { tabs ->
            tabs.setLocalTabs(
                listOf(
                    RemoteTab(
                        title = "cool things to look at in your remote tabs",
                        urlHistory = listOf("https://example.com"),
                        icon = null,
                        lastUsed = 0,
                    ),
                    RemoteTab(
                        title = "cool things to look at in your remote tabs",
                        urlHistory = listOf(),
                        icon = null,
                        lastUsed = 12,
                        inactive = true,
                    ),
                ),
            )
        }
    }

    @Test
    fun getAllTest() {
        val store = getTestStore()
        store.getAll()
    }

    @Test
    fun testRegisterWithSyncmanager() {
        val syncManager = SyncManager()

        Assert.assertFalse(syncManager.getAvailableEngines().contains("tabs"))

        getTestStore().registerWithSyncManager()
        Assert.assertTrue(syncManager.getAvailableEngines().contains("tabs"))
    }
}
