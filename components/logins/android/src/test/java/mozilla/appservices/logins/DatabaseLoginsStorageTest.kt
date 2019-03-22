/* Any copyright is dedicated to the Public Domain.
   http://creativecommons.org/publicdomain/zero/1.0/ */

package mozilla.appservices.logins

import org.junit.rules.TemporaryFolder
import org.junit.Rule
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.junit.Test
import org.junit.Assert.assertNotNull
import org.junit.Assert.fail

@RunWith(RobolectricTestRunner::class)
@Config(manifest = Config.NONE)
class DatabaseLoginsStorageTest : LoginsStorageTest() {
    @Rule
    @JvmField
    val dbFolder = TemporaryFolder()

    override fun createTestStore(): LoginsStorage {
        val dbPath = dbFolder.newFile()
        return DatabaseLoginsStorage(dbPath = dbPath.absolutePath)
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
}
