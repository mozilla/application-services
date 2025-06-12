/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.logins

import androidx.test.core.app.ApplicationProvider
import kotlinx.coroutines.test.runTest
import mozilla.appservices.RustComponentsInitializer
import mozilla.appservices.syncmanager.SyncManager
import mozilla.telemetry.glean.testing.GleanTestRule
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(manifest = Config.NONE)
class DatabaseLoginsStorageTest {
    @Rule
    @JvmField
    val dbFolder = TemporaryFolder()

    @get:Rule
    val gleanRule = GleanTestRule(ApplicationProvider.getApplicationContext())

    fun createTestStore(): DatabaseLoginsStorage {
        RustComponentsInitializer.init()

        val dbPath = dbFolder.newFile()
        val encryptionKey = createKey()
        val keyManager = createStaticKeyManager(key = encryptionKey)
        return DatabaseLoginsStorage(dbPath = dbPath.absolutePath, keyManager = keyManager)
    }

    private suspend fun getTestStore(): DatabaseLoginsStorage {
        val store = createTestStore()

        store.add(
            LoginEntry(
                origin = "https://www.example.com",
                httpRealm = "Something",
                usernameField = "users_name",
                passwordField = "users_password",
                formActionOrigin = null,
                username = "Foobar2000",
                password = "hunter2",
            ),
        )

        store.add(
            LoginEntry(
                origin = "https://www.example.org",
                httpRealm = "",
                formActionOrigin = "https://www.example.org/login",
                usernameField = "users_name",
                passwordField = "users_password",
                password = "MyVeryCoolPassword",
                username = "Foobar2000",
            ),
        )

        return store
    }

    private fun finishAndClose(store: DatabaseLoginsStorage) {
        store.close()
        // if this is all we need to do, then this helper should die!
    }

     @Test
     fun testTouch() = runTest {
        val store = getTestStore()
        val login = store.list()[0]
        // Wait 100ms so that touch is certain to change timeLastUsed.
        Thread.sleep(100)
        store.touch(login.id)

        val updatedLogin = store.get(login.id)

        assertNotNull(updatedLogin)
        assertEquals(login.timesUsed + 1, updatedLogin!!.timesUsed)
        assert(updatedLogin.timeLastUsed > login.timeLastUsed)

        try {
            store.touch("abcdabcdabcd")
        } catch (e: LoginsApiException.NoSuchRecord) {
            // Expected error
        }

        finishAndClose(store)
    }

    @Test
    fun testDelete() = runTest {
        val store = getTestStore()
        val login = store.list()[0]

        assertNotNull(store.get(login.id))
        assertTrue(store.delete(login.id))
        assertNull(store.get(login.id))
        assertFalse(store.delete(login.id))
        assertNull(store.get(login.id))

        finishAndClose(store)
    }

    @Test
    fun testWipeLocal() = runTest {
        val test = getTestStore()
        val logins = test.list()
        assertEquals(2, logins.size)

        test.wipeLocal()
        assertEquals(0, test.list().size)

        assertNull(test.get(logins[0].id))
        assertNull(test.get(logins[1].id))

        finishAndClose(test)
    }

    @Test
    fun testRegisterWithSyncmanager() {
        val store = createTestStore()
        val syncManager = SyncManager()

        assertFalse(syncManager.getAvailableEngines().contains("passwords"))

        store.registerWithSyncManager()
        assertTrue(syncManager.getAvailableEngines().contains("passwords"))
    }
}
