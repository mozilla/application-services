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
import org.junit.Assert.fail
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.junit.runner.RunWith
import org.mozilla.appservices.logins.GleanMetrics.LoginsStore as LoginsStoreMetrics
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
        Megazord.init()
        val dbPath = dbFolder.newFile()
        return DatabaseLoginsStorage(dbPath = dbPath.absolutePath)
    }

    protected val encryptionKey = "testEncryptionKey"

    protected fun getTestStore(): LoginsStorage {
        val store = createTestStore()

        store.unlock(encryptionKey)

        store.add(ServerPassword(
                id = "aaaaaaaaaaaa",
                hostname = "https://www.example.com",
                httpRealm = "Something",
                username = "Foobar2000",
                password = "hunter2",
                usernameField = "users_name",
                passwordField = "users_password"
        ))

        store.add(ServerPassword(
                id = "bbbbbbbbbbbb",
                username = "Foobar2000",
                hostname = "https://www.example.org",
                formSubmitURL = "https://www.example.org/login",
                password = "MyVeryCoolPassword",
                usernameField = "users_name",
                passwordField = "users_password"
        ))

        store.lock()
        return store
    }

    protected fun finishAndClose(store: LoginsStorage) {
        store.ensureLocked()
        assertEquals(store.isLocked(), true)
        store.close()
    }

    protected inline fun <T : Any?, reified E : Throwable> expectException(klass: Class<E>, callback: () -> T) {
        try {
            callback()
            fail("Expected exception!")
        } catch (e: Throwable) {
            assert(klass.isInstance(e), { "Expected $klass but got exception of type ${e.javaClass}: $e" })
        }
    }

    @Test
    fun testUnlockHex() {
        val store = createTestStore()
        val key = "0123456789abcdef"
        // This is a little awkward because kotlin/java Byte is signed, and so the literals
        // above 128 (above 0x80) can't be part of a `listOf<Byte>()` (there's UByte, but it's
        // both experimental and very unclear that JNA would do anything sane with it).
        val keyBytes = listOf(0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef)
                .map { it.toByte() }
                .toByteArray()

        store.unlock(keyBytes)

        store.add(ServerPassword(
                id = "aaaaaaaaaaaa",
                hostname = "https://www.example.com",
                httpRealm = "Something",
                username = "Foobar2000",
                password = "hunter2",
                usernameField = "users_name",
                passwordField = "users_password"
        ))

        store.lock()
        // Ensure that it's equivalent to encrypting with the hex encoded string.
        store.unlock(key)

        assertNotNull(store.get("aaaaaaaaaaaa"))

        store.lock()
        // Check that the wrong key fails
        try {
            store.unlock(listOf<Byte>(0x01, 0x02, 0x03).toByteArray())
            fail("Should have thrown")
        } catch (e: InvalidKeyException) {
            // All good.
        }
        assert(store.isLocked())
        // Make sure that ensureUnlocked works when locked or unlocked
        store.ensureUnlocked(keyBytes)
        assert(!store.isLocked())

        store.ensureUnlocked(keyBytes)
        assert(!store.isLocked())

        finishAndClose(store)
    }

    @Test
    fun testSyncException() {
        val test = getTestStore()
        test.ensureUnlocked(encryptionKey)
        // Make sure we throw the right exception for invalid info.
        expectException(SyncAuthInvalidException::class.java) {
            // Provide a real URL for the tokenserver, or we give back an unexpected error about it being an invalid URL
            test.sync(SyncUnlockInfo(
                    kid = "",
                    fxaAccessToken = "",
                    syncKey = "",
                    tokenserverURL = "https://asdf.com"
            ))
        }

        finishAndClose(test)
    }

    @Test
    fun testMetricsGathering() {
        val store = createTestStore()
        val key = "0123456789abcdef"

        assert(!LoginsStoreMetrics.unlockTime.testHasValue())
        assert(!LoginsStoreMetrics.unlockCount.testHasValue())
        assert(!LoginsStoreMetrics.unlockErrorCount["invalid_key"].testHasValue())

        store.unlock(key)

        assert(LoginsStoreMetrics.unlockTime.testHasValue())
        assertEquals(LoginsStoreMetrics.unlockCount.testGetValue(), 1)
        assert(!LoginsStoreMetrics.unlockErrorCount["invalid_key"].testHasValue())

        store.lock()
        try {
            store.unlock(listOf<Byte>(0x01, 0x02, 0x03).toByteArray())
            fail("Should have thrown")
        } catch (e: InvalidKeyException) {
            // All good.
        }
        store.unlock(key)

        assert(LoginsStoreMetrics.unlockTime.testHasValue())
        assertEquals(LoginsStoreMetrics.unlockCount.testGetValue(), 3)
        assert(LoginsStoreMetrics.unlockErrorCount["invalid_key"].testHasValue())
        assertEquals(LoginsStoreMetrics.unlockErrorCount["invalid_key"].testGetValue(), 1)

        assert(!LoginsStoreMetrics.writeQueryTime.testHasValue())
        assert(!LoginsStoreMetrics.writeQueryCount.testHasValue())
        assert(!LoginsStoreMetrics.writeQueryErrorCount["invalid_record"].testHasValue())

        store.add(ServerPassword(
                id = "aaaaaaaaaaaa",
                hostname = "https://www.example.com",
                httpRealm = "Something",
                username = "Foobar2000",
                password = "hunter2",
                usernameField = "users_name",
                passwordField = "users_password"
        ))

        assert(LoginsStoreMetrics.writeQueryTime.testHasValue())
        assertEquals(LoginsStoreMetrics.writeQueryCount.testGetValue(), 1)
        assert(!LoginsStoreMetrics.writeQueryErrorCount["invalid_record"].testHasValue())

        // N.B. this is invalid due to `formSubmitURL` being an invalid url.
        val invalid = ServerPassword(
            id = "bbbbbbbbbbbb",
            hostname = "https://test.example.com",
            formSubmitURL = "not a url",
            username = "Foobar2000",
            password = "hunter2",
            usernameField = "users_name",
            passwordField = "users_password"
        )

        try {
            store.add(invalid)
            fail("Should have thrown")
        } catch (e: InvalidRecordException) {
            // All good.
        }

        assert(LoginsStoreMetrics.writeQueryTime.testHasValue())
        assertEquals(LoginsStoreMetrics.writeQueryCount.testGetValue(), 2)
        assertEquals(LoginsStoreMetrics.writeQueryErrorCount["invalid_record"].testGetValue(), 1)

        assert(!LoginsStoreMetrics.readQueryTime.testHasValue())
        assert(!LoginsStoreMetrics.readQueryCount.testHasValue())
        assert(!LoginsStoreMetrics.readQueryErrorCount["storage_error"].testHasValue())

        val record = store.get("aaaaaaaaaaaa")!!
        assertEquals(record.hostname, "https://www.example.com")

        assert(LoginsStoreMetrics.readQueryTime.testHasValue())
        assertEquals(LoginsStoreMetrics.readQueryCount.testGetValue(), 1)
        assert(!LoginsStoreMetrics.readQueryErrorCount["storage_error"].testHasValue())

        // Ensure that ensureValid doesn't cause us to record invalid_record errors.
        try {
            store.ensureValid(invalid)
            fail("Should have thrown")
        } catch (e: InvalidRecordException) {
            // All good.
        }

        assert(LoginsStoreMetrics.readQueryTime.testHasValue())
        assertEquals(LoginsStoreMetrics.readQueryCount.testGetValue(), 2)
        assert(!LoginsStoreMetrics.readQueryErrorCount["invalid_record"].testHasValue())

        finishAndClose(store)
    }

    @Test
    fun testLockedOperations() {
        val test = getTestStore()
        assertEquals(test.isLocked(), true)

        expectException(LoginsStorageException::class.java) { test.get("aaaaaaaaaaaa") }
        expectException(LoginsStorageException::class.java) { test.list() }
        expectException(LoginsStorageException::class.java) { test.delete("aaaaaaaaaaaa") }
        expectException(LoginsStorageException::class.java) { test.touch("bbbbbbbbbbbb") }
        expectException(LoginsStorageException::class.java) { test.wipe() }
        expectException(LoginsStorageException::class.java) { test.sync(SyncUnlockInfo("", "", "", "")) }
        expectException(LoginsStorageException::class.java) {
            @Suppress("DEPRECATION")
            test.reset()
        }

        test.unlock(encryptionKey)
        assertEquals(test.isLocked(), false)
        // Make sure things didn't change despite being locked
        assertNotNull(test.get("aaaaaaaaaaaa"))
        // "bbbbbbbbbbbb" has a single use (from insertion)
        assertEquals(1, test.get("bbbbbbbbbbbb")!!.timesUsed)
        finishAndClose(test)
    }

    @Test
    fun testEnsureLockUnlock() {
        val test = getTestStore()
        assertEquals(test.isLocked(), true)

        test.ensureUnlocked(encryptionKey)
        assertEquals(test.isLocked(), false)
        test.ensureUnlocked(encryptionKey)
        assertEquals(test.isLocked(), false)

        test.ensureLocked()
        assertEquals(test.isLocked(), true)
        test.ensureLocked()
        assertEquals(test.isLocked(), true)

        finishAndClose(test)
    }

    @Test
    fun testTouch() {
        val test = getTestStore()
        test.unlock(encryptionKey)
        assertEquals(test.list().size, 2)
        val b = test.get("bbbbbbbbbbbb")!!

        // Wait 100ms so that touch is certain to change timeLastUsed.
        Thread.sleep(100)
        test.touch("bbbbbbbbbbbb")

        val newB = test.get("bbbbbbbbbbbb")

        assertNotNull(newB)
        assertEquals(b.timesUsed + 1, newB!!.timesUsed)
        assert(newB.timeLastUsed > b.timeLastUsed)

        expectException(NoSuchRecordException::class.java) { test.touch("abcdabcdabcd") }

        finishAndClose(test)
    }

    @Test
    fun testDelete() {
        val test = getTestStore()

        test.unlock(encryptionKey)
        assertNotNull(test.get("aaaaaaaaaaaa"))
        assertTrue(test.delete("aaaaaaaaaaaa"))
        assertNull(test.get("aaaaaaaaaaaa"))
        assertFalse(test.delete("aaaaaaaaaaaa"))
        assertNull(test.get("aaaaaaaaaaaa"))

        finishAndClose(test)
    }

    @Test
    fun testListWipe() {
        val test = getTestStore()
        test.unlock(encryptionKey)
        assertEquals(2, test.list().size)

        test.wipe()
        assertEquals(0, test.list().size)

        assertNull(test.get("aaaaaaaaaaaa"))
        assertNull(test.get("bbbbbbbbbbbb"))

        finishAndClose(test)
    }

    @Test
    fun testWipeLocal() {
        val test = getTestStore()
        test.unlock(encryptionKey)
        assertEquals(2, test.list().size)

        test.wipeLocal()
        assertEquals(0, test.list().size)

        assertNull(test.get("aaaaaaaaaaaa"))
        assertNull(test.get("bbbbbbbbbbbb"))

        finishAndClose(test)
    }

    @Test
    fun testAdd() {
        val test = getTestStore()
        test.unlock(encryptionKey)

        expectException(IdCollisionException::class.java) {
            test.add(ServerPassword(
                    id = "aaaaaaaaaaaa",
                    hostname = "https://www.foo.org",
                    httpRealm = "Some Realm",
                    password = "MyPassword",
                    username = "MyUsername",
                    usernameField = "",
                    passwordField = ""
            ))
        }

        for (record in INVALID_RECORDS) {
            expectException(InvalidRecordException::class.java) {
                test.add(record)
            }
        }

        val toInsert = ServerPassword(
                id = "",
                hostname = "https://www.foo.org",
                httpRealm = "Some Realm",
                password = "MyPassword",
                username = "Foobar2000",
                usernameField = "",
                passwordField = ""
        )

        val generatedID = test.add(toInsert)

        val record = test.get(generatedID)!!
        assertEquals(generatedID, record.id)
        assertEquals(toInsert.hostname, record.hostname)
        assertEquals(toInsert.httpRealm, record.httpRealm)
        assertEquals(toInsert.password, record.password)
        assertEquals(toInsert.username, record.username)
        assertEquals(toInsert.passwordField, record.passwordField)
        assertEquals(toInsert.usernameField, record.usernameField)
        assertEquals(toInsert.formSubmitURL, record.formSubmitURL)
        assertEquals(1, record.timesUsed)

        assertNotEquals(0L, record.timeLastUsed)
        assertNotEquals(0L, record.timeCreated)
        assertNotEquals(0L, record.timePasswordChanged)

        val specificID = test.add(ServerPassword(
                id = "123412341234",
                hostname = "http://www.bar.com",
                formSubmitURL = "http://login.bar.com",
                password = "DummyPassword",
                username = "DummyUsername",
                usernameField = "users_name",
                passwordField = "users_password"
        ))

        assertEquals("123412341234", specificID)

        finishAndClose(test)
    }

    @Test
    fun testEnsureValid() {
        val test = getTestStore()
        test.unlock(encryptionKey)

        test.add(ServerPassword(
                id = "bbbbb",
                hostname = "https://www.foo.org",
                httpRealm = "Some Realm",
                password = "MyPassword",
                username = "MyUsername",
                usernameField = "",
                passwordField = ""
        ))

        val dupeLogin = ServerPassword(
                id = "",
                hostname = "https://www.foo.org",
                httpRealm = "Some Realm",
                password = "MyPassword",
                username = "MyUsername",
                usernameField = "",
                passwordField = ""
        )

        val nullValueLogin = ServerPassword(
                id = "",
                hostname = "https://www.test.org",
                httpRealm = "Some Other Realm",
                password = "MyPassword",
                username = "\u0000MyUsername2",
                usernameField = "",
                passwordField = ""
        )

        expectException(InvalidRecordException::class.java) {
            test.ensureValid(dupeLogin)
        }

        expectException(InvalidRecordException::class.java) {
            test.ensureValid(nullValueLogin)
        }

        test.delete("bbbbb")
    }

    @Test
    fun testPotentialDupesIgnoringUsername() {
        val test = getTestStore()
        test.unlock(encryptionKey)

        val savedLogin1 = ServerPassword(
                id = "bbbbb",
                hostname = "https://www.foo.org",
                httpRealm = "Some Realm",
                password = "MyPassword",
                username = "MyUsername",
                usernameField = "",
                passwordField = ""
        )

        test.add(savedLogin1)

        val dupeLogin = ServerPassword(
                id = "",
                hostname = "https://www.foo.org",
                httpRealm = "Some Realm",
                password = "MyPassword",
                username = "MySecondUsername",
                usernameField = "",
                passwordField = ""
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

        expectException(NoSuchRecordException::class.java) {
            test.update(ServerPassword(
                    id = "123412341234",
                    hostname = "https://www.foo.org",
                    httpRealm = "Some Realm",
                    password = "MyPassword",
                    username = "MyUsername",
                    usernameField = "",
                    passwordField = ""
            ))
        }

        for (record in INVALID_RECORDS) {
            val updateArg = record.copy(id = "aaaaaaaaaaaa")
            expectException(InvalidRecordException::class.java) {
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
        assertEquals(toUpdate.hostname, record.hostname)
        assertEquals(toUpdate.httpRealm, record.httpRealm)
        assertEquals(toUpdate.password, record.password)
        assertEquals(toUpdate.username, record.username)
        assertEquals(toUpdate.passwordField, record.passwordField)
        assertEquals(toUpdate.usernameField, record.usernameField)
        assertEquals(toUpdate.formSubmitURL, record.formSubmitURL)
        assertEquals(toUpdate.timesUsed + 1, record.timesUsed)
        assertEquals(toUpdate.timeCreated, record.timeCreated)

        assert(toUpdate.timeLastUsed < record.timeLastUsed)

        assert(toUpdate.timeLastUsed < record.timeLastUsed)
        assert(toUpdate.timeLastUsed < record.timePasswordChanged)

        val specificID = test.add(ServerPassword(
                id = "123412341234",
                hostname = "http://www.bar.com",
                formSubmitURL = "http://login.bar.com",
                password = "DummyPassword",
                username = "DummyUsername",
                usernameField = "users_name",
                passwordField = "users_password"
        ))

        assertEquals("123412341234", specificID)

        finishAndClose(test)
    }

    @Test
    @Suppress("DEPRECATION")
    fun testUnlockAfterError() {
        val test = getTestStore()

        expectException(LoginsStorageException::class.java) {
            test.reset()
        }

        test.unlock(encryptionKey)

        test.reset()

        finishAndClose(test)
    }

    companion object {
        val INVALID_RECORDS: List<ServerPassword> = listOf(
                // Invalid formSubmitURL
                ServerPassword(
                        id = "",
                        hostname = "https://www.foo.org",
                        formSubmitURL = "invalid\u0000value",
                        password = "MyPassword",
                        username = "MyUsername",
                        usernameField = "users_name",
                        passwordField = "users_password"
                ),
                // Neither formSubmitURL nor httpRealm
                ServerPassword(
                        id = "",
                        hostname = "https://www.foo.org",
                        password = "MyPassword",
                        username = "MyUsername",
                        usernameField = "",
                        passwordField = ""
                ),
                // Empty password
                ServerPassword(
                        id = "",
                        hostname = "https://www.foo.org",
                        httpRealm = "Some Realm",
                        password = "",
                        username = "MyUsername",
                        usernameField = "",
                        passwordField = ""
                ),
                // Empty hostname
                ServerPassword(
                        id = "",
                        hostname = "",
                        httpRealm = "Some Realm",
                        password = "MyPassword",
                        username = "MyUsername",
                        usernameField = "",
                        passwordField = ""
                )
        )
    }
}
