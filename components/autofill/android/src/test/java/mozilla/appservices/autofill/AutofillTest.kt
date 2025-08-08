/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import mozilla.appservices.autofill.Store
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
class AutofillTest {
    @Rule
    @JvmField
    val dbFolder = TemporaryFolder()

    fun createTestStore(): Store {
        val dbPath = dbFolder.newFile()
        return Store(dbpath = dbPath.absolutePath)
    }

    @Test
    fun testRegisterWithSyncmanager() {
        val syncManager = SyncManager()

        Assert.assertFalse(syncManager.getAvailableEngines().contains("addresses"))
        Assert.assertFalse(syncManager.getAvailableEngines().contains("creditcards"))

        createTestStore().registerWithSyncManager()

        Assert.assertTrue(syncManager.getAvailableEngines().contains("addresses"))
        Assert.assertTrue(syncManager.getAvailableEngines().contains("creditcards"))
    }
}
