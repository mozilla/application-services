/* Any copyright is dedicated to the Public Domain.
   http://creativecommons.org/publicdomain/zero/1.0/ */

package mozilla.appservices.logins

import org.junit.Test
import org.junit.Assert.*
import java.util.concurrent.CountDownLatch
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

abstract class LoginsStorageTest {

    abstract fun createTestStore(): LoginsStorage

    private val encryptionKey = "testEncryptionKey"

    private fun getTestStore(): LoginsStorage {
        val store = createTestStore()

        waitForResult(store.unlock(encryptionKey))

        waitForResult(store.add(ServerPassword(
                id = "aaaaaaaaaaaa",
                hostname = "https://www.example.com",
                httpRealm = "Something",
                username = "Foobar2000",
                password = "hunter2",
                usernameField = "users_name",
                passwordField = "users_password"
        )))

        waitForResult(store.add(ServerPassword(
                id = "bbbbbbbbbbbb",
                hostname = "https://www.example.org",
                formSubmitURL = "https://www.example.org/login",
                password = "MyVeryCoolPassword"
        )))

        waitForResult(store.lock())
        return store
    }

    @Suppress("UNCHECKED_CAST")
    private fun <T> waitForResult(e: SyncResult<T>): T {
        val result: AtomicReference<T?> = AtomicReference(null)
        val finished = AtomicBoolean(false) // Needed since T may be nullable
        val gate = CountDownLatch(1)
        e.thenCatch { err ->
            fail(err.message)
            gate.countDown()

            throw err // Needed to typecheck, kotlin doesn't know that fail() doesn't return
        }.whenComplete {
            result.set(it)
            if (finished.getAndSet(true)) {
                fail("Timed out!")
            }
            gate.countDown()
        }
        gate.await()
        val v = result.get()
        assert(finished.get())
        return v as T
    }

    private fun <T> waitForException(e: SyncResult<T>): Exception {
        val result: AtomicReference<Exception?> = AtomicReference(null)
        val gate = CountDownLatch(1)
        e.then({
            fail("Expected exception but resolved successfully")
            SyncResult.fromValue(Unit)
        }) { err ->
            if (result.getAndSet(err) != null) {
                // Probably something will have killed this thread by this point.
                fail("Timed Out!")
            }
            gate.countDown()
            SyncResult.fromValue(Unit)
        }

        gate.await()
        val v = result.get()
        assertNotNull(v)
        return v!!
    }

    private fun finishAndClose(store: LoginsStorage) {
        waitForResult(store.lock())
        assertEquals(waitForResult(store.isLocked()), true)
        store.close()
    }

    @Test
    fun testLockedOperations() {
        val test = getTestStore()
        assertEquals(waitForResult(test.isLocked()), true)
        // Note that waitForException fails the test if it successfully resolves
        waitForException(test.get("aaaaaaaaaaaa"))
        waitForException(test.list())
        waitForException(test.delete("aaaaaaaaaaaa"))
        waitForException(test.touch("bbbbbbbbbbbb"))
        waitForException(test.wipe())
        waitForException(test.sync(SyncUnlockInfo("", "", "", "")))
        waitForException(test.reset())

        waitForResult(test.unlock(encryptionKey))
        assertEquals(waitForResult(test.isLocked()), false)
        // Make sure things didn't change despite being locked
        assertNotNull(waitForResult(test.get("aaaaaaaaaaaa")))
        // "bbbbbbbbbbbb" has a single use (from insertion)
        assertEquals(1, waitForResult(test.get("bbbbbbbbbbbb"))!!.timesUsed)
        finishAndClose(test)
    }

    @Test
    fun testTouch() {
        val test = getTestStore()
        waitForResult(test.unlock(encryptionKey))
        assertEquals(waitForResult(test.list()).size, 2)
        val b = waitForResult(test.get("bbbbbbbbbbbb"))!!

        // Wait 100ms so that touch is certain to change timeLastUsed.
        Thread.sleep(100)
        waitForResult(test.touch("bbbbbbbbbbbb"))

        val newB = waitForResult(test.get("bbbbbbbbbbbb"))

        assertNotNull(newB)
        assertEquals(b.timesUsed + 1, newB!!.timesUsed)
        assert(newB.timeLastUsed > b.timeLastUsed)

        val exn = waitForException(test.touch("abcdabcdabcd"))
        assert(exn is NoSuchRecordException)

        finishAndClose(test)
    }

    @Test
    fun testDelete() {
        val test = getTestStore()

        waitForResult(test.unlock(encryptionKey))
        assertNotNull(waitForResult(test.get("aaaaaaaaaaaa")))
        assertTrue(waitForResult(test.delete("aaaaaaaaaaaa")))
        assertNull(waitForResult(test.get("aaaaaaaaaaaa")))
        assertFalse(waitForResult(test.delete("aaaaaaaaaaaa")))
        assertNull(waitForResult(test.get("aaaaaaaaaaaa")))

        finishAndClose(test)
    }

    @Test
    fun testListWipe() {
        val test = getTestStore()
        waitForResult(test.unlock(encryptionKey))
        assertEquals(2, waitForResult(test.list()).size)

        waitForResult(test.wipe())
        assertEquals(0, waitForResult(test.list()).size)

        assertNull(waitForResult(test.get("aaaaaaaaaaaa")))
        assertNull(waitForResult(test.get("bbbbbbbbbbbb")))

        finishAndClose(test)
    }

    @Test
    fun testAdd() {
        val test = getTestStore()
        waitForResult(test.unlock(encryptionKey))

        assert(waitForException(test.add(ServerPassword(
                id = "aaaaaaaaaaaa",
                hostname = "https://www.foo.org",
                httpRealm = "Some Realm",
                password = "MyPassword",
                username = "MyUsername"
        ))) is IdCollisionException)

        for (record in INVALID_RECORDS) {
            assert(waitForException(test.add(record)) is InvalidRecordException,
                    { "Expected InvalidRecordException adding $record" })
        }

        val toInsert = ServerPassword(
                id = "",
                hostname = "https://www.foo.org",
                httpRealm = "Some Realm",
                password = "MyPassword",
                username = null
        )

        val generatedID = waitForResult(test.add(toInsert))

        val record = waitForResult(test.get(generatedID))!!
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

        val specificID = waitForResult(test.add(ServerPassword(
                id = "123412341234",
                hostname = "http://www.bar.com",
                formSubmitURL = "http://login.bar.com",
                password = "DummyPassword",
                username = "DummyUsername")))

        assertEquals("123412341234", specificID)

        finishAndClose(test)
    }

    @Test
    fun testUpdate() {
        val test = getTestStore()
        waitForResult(test.unlock(encryptionKey))

        assert(waitForException(test.update(ServerPassword(
                id = "123412341234",
                hostname = "https://www.foo.org",
                httpRealm = "Some Realm",
                password = "MyPassword",
                username = "MyUsername"
        ))) is NoSuchRecordException)

        for (record in INVALID_RECORDS) {
            val updateArg = record.copy(id = "aaaaaaaaaaaa")
            assert(waitForException(test.update(updateArg)) is InvalidRecordException,
                    { "Expected InvalidRecordException updating $updateArg" })
        }

        val toUpdate = waitForResult(test.get("aaaaaaaaaaaa"))!!.copy(
            password = "myNewPassword"
        )

        // Sleep so that the current time for test.update is guaranteed to be
        // different.
        Thread.sleep(100)

        waitForResult(test.update(toUpdate))


        val record = waitForResult(test.get(toUpdate.id))!!
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

        val specificID = waitForResult(test.add(ServerPassword(
                id = "123412341234",
                hostname = "http://www.bar.com",
                formSubmitURL = "http://login.bar.com",
                password = "DummyPassword",
                username = "DummyUsername")))

        assertEquals("123412341234", specificID)

        finishAndClose(test)
    }

    companion object {
        val INVALID_RECORDS: List<ServerPassword> = listOf(
            // Both formSubmitURL and httpRealm
                ServerPassword(
                        id = "",
                        hostname = "https://www.foo.org",
                        httpRealm = "Test Realm",
                        formSubmitURL = "https://www.foo.org/login",
                        password = "MyPassword",
                        username = "MyUsername"
                ),
            // Neither formSubmitURL nor httpRealm
                ServerPassword(
                        id = "",
                        hostname = "https://www.foo.org",
                        password = "MyPassword",
                        username = "MyUsername"
                ),
            // Empty password
                ServerPassword(
                        id = "",
                        hostname = "https://www.foo.org",
                        httpRealm = "Some Realm",
                        password = "",
                        username = "MyUsername"
                ),
            // Empty hostname
                ServerPassword(
                        id = "",
                        hostname = "",
                        httpRealm = "Some Realm",
                        password = "MyPassword",
                        username = "MyUsername"
                )
        )
    }
}
