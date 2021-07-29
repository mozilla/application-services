/* Any copyright is dedicated to the Public Domain.
   http://creativecommons.org/publicdomain/zero/1.0/ */

package mozilla.appservices.logins

import androidx.test.core.app.ApplicationProvider
import mozilla.appservices.Megazord
import mozilla.components.service.glean.testing.GleanTestRule
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.assertThrows
import org.junit.Assert.fail
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.junit.runner.RunWith
import org.mozilla.appservices.logins.GleanMetrics.LoginsStore as LoginsStoreMetrics
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

// XXX - so yeah, lots to do here still :(
// This test file compiles :) It doesn't pass.
// Even after fixing the big commented-out block below is done, another challenge
// will be fetching records with hard-coded GUIDs - eg:
// > val b = test.get("bbbbbbbbbbbb")!!
// fails because we no longer specify the GUID when adding. We'll have to work out
// how to remember the IDs of the test-records we add.

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

        store.add(UpdatableLogin(
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
        ), encryptionKey)

        store.add(UpdatableLogin(
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
        ), encryptionKey)

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

        val login = store.add(UpdatableLogin(
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
        ), encryptionKey)

        assertEquals(LoginsStoreMetrics.writeQueryCount.testGetValue(), 1)
        assert(!LoginsStoreMetrics.writeQueryErrorCount["invalid_record"].testHasValue())

        // N.B. this is invalid due to `formActionOrigin` being an invalid url.
        val invalid = UpdatableLogin(
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
        } catch (e: LoginsStorageErrorException.InvalidRecord) {
            // All good.
        }

        assertEquals(LoginsStoreMetrics.writeQueryCount.testGetValue(), 2)
        assertEquals(LoginsStoreMetrics.writeQueryErrorCount["invalid_record"].testGetValue(), 1)

        assert(!LoginsStoreMetrics.readQueryCount.testHasValue())
        assert(!LoginsStoreMetrics.readQueryErrorCount["storage_error"].testHasValue())

        val record = store.get(login.id)!!
        assertEquals(record.fields.origin, "https://www.example.com")

        assertEquals(LoginsStoreMetrics.readQueryCount.testGetValue(), 1)
        assert(!LoginsStoreMetrics.readQueryErrorCount["storage_error"].testHasValue())

        // Ensure that ensureValid doesn't cause us to record invalid_record errors.
        try {
            store.ensureValid("", invalid, encryptionKey)
            fail("Should have thrown")
        } catch (e: LoginsStorageErrorException.InvalidRecord) {
            // All good.
        }

        assertEquals(LoginsStoreMetrics.readQueryCount.testGetValue(), 2)
        assert(!LoginsStoreMetrics.readQueryErrorCount["invalid_record"].testHasValue())

        finishAndClose(store)
    }

    @Test
    fun testTouch() {
        val store = getTestStore()
        val login = store.list()[0]
        // Wait 100ms so that touch is certain to change timeLastUsed.
        Thread.sleep(100)
        store.touch(login.id)

        val updatedLogin = store.get(login.id)

        assertNotNull(updatedLogin)
        assertEquals(login.timesUsed + 1, updatedLogin!!.timesUsed)
        assert(updatedLogin.timeLastUsed > login.timeLastUsed)

        assertThrows(LoginsStorageErrorException.NoSuchRecord::class.java) { store.touch("abcdabcdabcd") }

        finishAndClose(store)
    }

    @Test
    fun testDelete() {
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
    fun testListWipe() {
        val test = getTestStore()
        val logins = test.list()
        assertEquals(2, logins.size)

        test.wipe()
        assertEquals(0, test.list().size)

        assertNull(test.get(logins[0].id))
        assertNull(test.get(logins[1].id))

        finishAndClose(test)
    }

    @Test
    fun testWipeLocal() {
        val test = getTestStore()
        val logins = test.list()
        assertEquals(2, logins.size)

        test.wipeLocal()
        assertEquals(0, test.list().size)

        assertNull(test.get(logins[0].id))
        assertNull(test.get(logins[1].id))

        finishAndClose(test)
    }

// so yeah, as above, lots to do here still :(
/*
    @Test

    fun testAdd() {
        val test = getTestStore()

        for (record in INVALID_RECORDS) {
            assertThrows(LoginsStorageErrorException.InvalidRecord::class.java) {
                test.add(record)
            }
        }

        val toInsert = UpdatableLogin(
            fields = LoginFields(
                origin = "https://www.foo.org",
                httpRealm = "Some Realm",
                usernameField = "",
                passwordField = "",
                formActionOrigin = null
            ),
            secFields = SecureLoginFields(
                password = "MyPassword",
                username = "Foobar2000"
            )
        )

        val generatedID = test.add(toInsert, encryptionKey).id

        val record = test.get(generatedID)!!
        assertEquals(generatedID, record.id)
        assertEquals(toInsert.origin, record.fields.origin)
        assertEquals(toInsert.httpRealm, record.httpRealm)
        assertEquals(toInsert.password, record.password)
        assertEquals(toInsert.username, record.username)
        assertEquals(toInsert.passwordField, record.passwordField)
        assertEquals(toInsert.usernameField, record.usernameField)
        assertEquals(toInsert.formActionOrigin, record.formActionOrigin)
        assertEquals(1, record.timesUsed)

        assertNotEquals(0L, record.timeLastUsed)
        assertNotEquals(0L, record.timeCreated)
        assertNotEquals(0L, record.timePasswordChanged)

        val put = test.add(UpdatableLogin(
            fields = SecureLoginFields (
                origin = "http://www.bar.com",
                formActionOrigin = "http://login.bar.com",
                usernameField = "users_name",
                passwordField = "users_password",
                httpRealm = null
            ),
            secFields = SecureLoginFields(
                password = "DummyPassword",
                username = "DummyUsername"
            )
        ), encryptionKey).id
        val got = test.get(put.id)!!

        assertEquals(put, got)

        finishAndClose(test)
    }
    @Test
    fun testEnsureValid() {
        val test = getTestStore()

        test.add(UpdatableLogin(
                id = "bbbbb",
                origin = "https://www.foo.org",
                httpRealm = "Some Realm",
                password = "MyPassword",
                username = "MyUsername",
                usernameField = "",
                passwordField = "",
                formActionOrigin = null,
                timesUsed = 0,
                timeCreated = 0,
                timeLastUsed = 0,
                timePasswordChanged = 0
        ))

        val dupeLogin = UpdatableLogin(
                id = "",
                origin = "https://www.foo.org",
                httpRealm = "Some Realm",
                password = "MyPassword",
                username = "MyUsername",
                usernameField = "",
                passwordField = "",
                formActionOrigin = "",
                timesUsed = 0,
                timeCreated = 0,
                timeLastUsed = 0,
                timePasswordChanged = 0
        )

        val nullValueLogin = UpdatableLogin(
                id = "",
                origin = "https://www.test.org",
                httpRealm = "Some Other Realm",
                password = "MyPassword",
                username = "\u0000MyUsername2",
                usernameField = "",
                passwordField = "",
                formActionOrigin = "",
                timesUsed = 0,
                timeCreated = 0,
                timeLastUsed = 0,
                timePasswordChanged = 0
        )

        assertThrows(LoginsStorageErrorException.InvalidRecord::class.java) {
            test.ensureValid(dupeLogin)
        }

        assertThrows(LoginsStorageErrorException.InvalidRecord::class.java) {
            test.ensureValid(nullValueLogin)
        }

        test.delete("bbbbb")
    }

    @Test
    fun testPotentialDupesIgnoringUsername() {
        val test = getTestStore()
        test.unlock(encryptionKey)

        val savedLogin1 = UpdatableLogin(
                id = "bbbbb",
                origin = "https://www.foo.org",
                httpRealm = "Some Realm",
                password = "MyPassword",
                username = "MyUsername",
                usernameField = "",
                passwordField = "",
                                formActionOrigin = "",
                timesUsed = 0,
                timeCreated = 0,
                timeLastUsed = 0,
                timePasswordChanged = 0
        )

        test.add(savedLogin1)

        val dupeLogin = UpdatableLogin(
                id = "",
                origin = "https://www.foo.org",
                httpRealm = "Some Realm",
                password = "MyPassword",
                username = "MySecondUsername",
                usernameField = "",
                passwordField = "",
                                formActionOrigin = "",
                timesUsed = 0,
                timeCreated = 0,
                timeLastUsed = 0,
                timePasswordChanged = 0
        )

        val potentialDupes = test.potentialDupesIgnoringUsername(dupeLogin)
        assert(potentialDupes.size == 1)
        assertEquals(potentialDupes[0].id, savedLogin1.id)

        test.delete("bbbbb")
    }

    @Test
    fun testUpdate() {
        val test = getTestStore()
        test.unlock(encryptionKey)

        assertThrows(LoginsStorageErrorException.NoSuchRecord::class.java) {
            test.update(UpdatableLogin(
                    id = "123412341234",
                    origin = "https://www.foo.org",
                    httpRealm = "Some Realm",
                    password = "MyPassword",
                    username = "MyUsername",
                    usernameField = "",
                    passwordField = "",
                    formActionOrigin = "",
                        timesUsed = 0,
                        timeCreated = 0,
                        timeLastUsed = 0,
                        timePasswordChanged = 0
            ))
        }

        for (record in INVALID_RECORDS) {
            val updateArg = record.copy(id = "aaaaaaaaaaaa")
            assertThrows(LoginsStorageErrorException.InvalidRecord::class.java) {
                test.update(updateArg)
            }
        }

        val toUpdate = test.get("aaaaaaaaaaaa")!!.copy(
                password = "myNewPassword"
        )

        // Sleep so that the current time for test.update is guaranteed to be
        // different.
        Thread.sleep(100)

        test.update(toUpdate)

        val record = test.get(toUpdate.id)!!
        assertEquals(toUpdate.origin, record.fields.origin)
        assertEquals(toUpdate.httpRealm, record.httpRealm)
        assertEquals(toUpdate.password, record.password)
        assertEquals(toUpdate.username, record.username)
        assertEquals(toUpdate.passwordField, record.passwordField)
        assertEquals(toUpdate.usernameField, record.usernameField)
        assertEquals(toUpdate.formActionOrigin, record.formActionOrigin)
        assertEquals(toUpdate.timesUsed + 1, record.timesUsed)
        assertEquals(toUpdate.timeCreated, record.timeCreated)

        assert(toUpdate.timeLastUsed < record.timeLastUsed)

        assert(toUpdate.timeLastUsed < record.timeLastUsed)
        assert(toUpdate.timeLastUsed < record.timePasswordChanged)

        val specificID = test.add(UpdatableLogin(
                id = "123412341234",
                origin = "http://www.bar.com",
                formActionOrigin = "http://login.bar.com",
                httpRealm = "",
                password = "DummyPassword",
                username = "DummyUsername",
                usernameField = "users_name",
                passwordField = "users_password",
                timesUsed = 0,
                timeCreated = 0,
                timeLastUsed = 0,
                timePasswordChanged = 0
        ))

        assertEquals("123412341234", specificID)

        finishAndClose(test)
    }

    companion object {
        val INVALID_RECORDS: List<Login> = listOf(
                // Invalid formActionOrigin
                UpdatableLogin(
                        id = "",
                        origin = "https://www.foo.org",
                        httpRealm = null,
                        formActionOrigin = "invalid\u0000value",
                        password = "MyPassword",
                        username = "MyUsername",
                        usernameField = "users_name",
                        passwordField = "users_password",
                        timesUsed = 0,
                        timeCreated = 0,
                        timeLastUsed = 0,
                        timePasswordChanged = 0
                ),
                // Neither formActionOrigin nor httpRealm
                UpdatableLogin(
                        id = "",
                        origin = "https://www.foo.org",
                        httpRealm = null,
                        password = "MyPassword",
                        username = "MyUsername",
                        usernameField = "",
                        passwordField = "",
                        formActionOrigin = null,
                        timesUsed = 0,
                        timeCreated = 0,
                        timeLastUsed = 0,
                        timePasswordChanged = 0
                ),
                // Empty password
                UpdatableLogin(
                        id = "",
                        origin = "https://www.foo.org",
                        httpRealm = "Some Realm",
                        password = "",
                        username = "MyUsername",
                        usernameField = "",
                        passwordField = "",
                        formActionOrigin = null,
                        timesUsed = 0,
                        timeCreated = 0,
                        timeLastUsed = 0,
                        timePasswordChanged = 0
                ),
                // Empty origin
                UpdatableLogin(
                        id = "",
                        origin = "",
                        httpRealm = "Some Realm",
                        password = "MyPassword",
                        username = "MyUsername",
                        usernameField = "",
                        passwordField = "",
                        formActionOrigin = null,
                        timesUsed = 0,
                        timeCreated = 0,
                        timeLastUsed = 0,
                        timePasswordChanged = 0
                )
        )
    }
*/

}
