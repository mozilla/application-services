/* Any copyright is dedicated to the Public Domain.
   http://creativecommons.org/publicdomain/zero/1.0/ */

package mozilla.appservices.logins

import androidx.test.core.app.ApplicationProvider
import mozilla.appservices.Megazord
import mozilla.appservices.syncmanager.SyncManager
import mozilla.components.service.glean.testing.GleanTestRule
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.mozilla.appservices.logins.GleanMetrics.LoginsStore as LoginsStoreMetrics

@RunWith(RobolectricTestRunner::class)
@Config(manifest = Config.NONE)
class DatabaseLoginsStorageTest {
    @Rule
    @JvmField
    val dbFolder = TemporaryFolder()

    @get:Rule
    val gleanRule = GleanTestRule(ApplicationProvider.getApplicationContext())

    fun createTestStore(): DatabaseLoginsStorage {
        Megazord.init()
        val dbPath = dbFolder.newFile()
        return DatabaseLoginsStorage(dbPath = dbPath.absolutePath)
    }

    protected val encryptionKey = createKey()

    protected fun getTestStore(): DatabaseLoginsStorage {
        val store = createTestStore()

        store.add(
            LoginEntry(
                fields = LoginFields(
                    origin = "https://www.example.com",
                    httpRealm = "Something",
                    usernameField = "users_name",
                    passwordField = "users_password",
                    formActionOrigin = null
                ),
                secFields = SecureLoginFields(
                    username = "Foobar2000",
                    password = "hunter2"
                )
            ),
            encryptionKey
        )

        store.add(
            LoginEntry(
                fields = LoginFields(
                    origin = "https://www.example.org",
                    httpRealm = "",
                    formActionOrigin = "https://www.example.org/login",
                    usernameField = "users_name",
                    passwordField = "users_password"
                ),
                secFields = SecureLoginFields(
                    password = "MyVeryCoolPassword",
                    username = "Foobar2000"
                )
            ),
            encryptionKey
        )

        return store
    }

    protected fun finishAndClose(store: DatabaseLoginsStorage) {
        store.close()
        // if this is all we need to do, then this helper should die!
    }

    @Test
    fun testMetricsGathering() {
        val store = createTestStore()

        assert(!LoginsStoreMetrics.writeQueryCount.testHasValue())
        assert(!LoginsStoreMetrics.writeQueryErrorCount["invalid_record"].testHasValue())

        val login = store.add(
            LoginEntry(
                fields = LoginFields(
                    origin = "https://www.example.com",
                    httpRealm = "Something",
                    usernameField = "users_name",
                    passwordField = "users_password",
                    formActionOrigin = null
                ),
                secFields = SecureLoginFields(
                    username = "Foobar2000",
                    password = "hunter2"
                )
            ),
            encryptionKey
        )

        assertEquals(LoginsStoreMetrics.writeQueryCount.testGetValue(), 1)
        assert(!LoginsStoreMetrics.writeQueryErrorCount["invalid_record"].testHasValue())

        // N.B. this is invalid due to `formActionOrigin` being an invalid url.
        val invalid = LoginEntry(
            fields = LoginFields(
                origin = "https://test.example.com",
                formActionOrigin = "not a url",
                httpRealm = "",
                usernameField = "users_name",
                passwordField = "users_password"
            ),
            secFields = SecureLoginFields(
                username = "Foobar2000",
                password = "hunter2"
            )
        )

        try {
            store.add(invalid, encryptionKey)
            fail("Should have thrown")
        } catch (e: LoginsStorageException.InvalidRecord) {
            // All good.
        }

        assertEquals(LoginsStoreMetrics.writeQueryCount.testGetValue(), 2)
        assertEquals(LoginsStoreMetrics.writeQueryErrorCount["invalid_record"].testGetValue(), 1)

        assert(!LoginsStoreMetrics.readQueryCount.testHasValue())
        assert(!LoginsStoreMetrics.readQueryErrorCount["storage_error"].testHasValue())

        val record = store.get(login.record.id)!!
        assertEquals(record.fields.origin, "https://www.example.com")

        assertEquals(LoginsStoreMetrics.readQueryCount.testGetValue(), 1)
        assert(!LoginsStoreMetrics.readQueryErrorCount["storage_error"].testHasValue())

        finishAndClose(store)
    }

    @Test
    fun testTouch() {
        val store = getTestStore()
        val login = store.list()[0]
        // Wait 100ms so that touch is certain to change timeLastUsed.
        Thread.sleep(100)
        store.touch(login.record.id)

        val updatedLogin = store.get(login.record.id)

        assertNotNull(updatedLogin)
        assertEquals(login.record.timesUsed + 1, updatedLogin!!.record.timesUsed)
        assert(updatedLogin.record.timeLastUsed > login.record.timeLastUsed)

        assertThrows(LoginsStorageException.NoSuchRecord::class.java) { store.touch("abcdabcdabcd") }

        finishAndClose(store)
    }

    @Test
    fun testDelete() {
        val store = getTestStore()
        val login = store.list()[0]

        assertNotNull(store.get(login.record.id))
        assertTrue(store.delete(login.record.id))
        assertNull(store.get(login.record.id))
        assertFalse(store.delete(login.record.id))
        assertNull(store.get(login.record.id))

        finishAndClose(store)
    }

    @Test
    fun testListWipe() {
        val test = getTestStore()
        val logins = test.list()
        assertEquals(2, logins.size)

        test.wipe()
        assertEquals(0, test.list().size)

        assertNull(test.get(logins[0].record.id))
        assertNull(test.get(logins[1].record.id))

        finishAndClose(test)
    }


    @Test
    fun testMigrationMetrics() {
        // We captured this string from one of the rust tests, then lightly
        // edited it. Note that none of the "phases" will ever have data.
        val json = """
            {"fixup_phase":{
                "num_processed":0,"num_succeeded":0,"num_failed":0,"total_duration":0,"errors":[]
            },
            "insert_phase":{"num_processed":0,"num_succeeded":0,"num_failed":0,"total_duration":0,"errors":[]
            },
            "num_processed":3,"num_succeeded":1,"num_failed":2,"total_duration":53,"errors":[
                "Invalid login: Login has illegal field: Origin is Malformed",
                "Invalid login: Origin is empty"
            ]
        }"""
        recordMigrationMetrics(json)
        assertEquals(3, LoginsStoreMetrics.migrationNumProcessed.testGetValue())
        assertEquals(2, LoginsStoreMetrics.migrationNumFailed.testGetValue())
        assertEquals(1, LoginsStoreMetrics.migrationNumSucceeded.testGetValue())
        assertEquals(53, LoginsStoreMetrics.migrationTotalDuration.testGetValue())
        // Note the truncation of the first error string.
        assertEquals(listOf("Invalid login: Login has illegal field: Origin is ", "Invalid login: Origin is empty"), LoginsStoreMetrics.migrationErrors.testGetValue())
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
