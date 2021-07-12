// /* Any copyright is dedicated to the Public Domain.
//    http://creativecommons.org/publicdomain/zero/1.0/ */

// package mozilla.appservices.logins

// import androidx.test.core.app.ApplicationProvider
// import mozilla.appservices.Megazord
// import mozilla.components.service.glean.testing.GleanTestRule
// import org.junit.Assert.assertEquals
// import org.junit.Assert.assertFalse
// import org.junit.Assert.assertNotNull
// import org.junit.Assert.assertNotEquals
// import org.junit.Assert.assertNull
// import org.junit.Assert.assertTrue
// import org.junit.Assert.assertThrows
// import org.junit.Assert.fail
// import org.junit.Rule
// import org.junit.Test
// import org.junit.rules.TemporaryFolder
// import org.junit.runner.RunWith
// import org.mozilla.appservices.logins.GleanMetrics.LoginsStore as LoginsStoreMetrics
// import org.robolectric.RobolectricTestRunner
// import org.robolectric.annotation.Config

// @RunWith(RobolectricTestRunner::class)
// @Config(manifest = Config.NONE)
// class DatabaseLoginsStorageTest {
//     @Rule
//     @JvmField
//     val dbFolder = TemporaryFolder()

//     @get:Rule
//     val gleanRule = GleanTestRule(ApplicationProvider.getApplicationContext())

//     fun createTestStore(): DatabaseLoginsStorage {
//         Megazord.init()
//         val dbPath = dbFolder.newFile()
//         return DatabaseLoginsStorage(dbPath = dbPath.absolutePath)
//     }

//     protected val encryptionKey = "testEncryptionKey"

//     protected fun getTestStore(): DatabaseLoginsStorage {
//         val store = createTestStore()

//         store.unlock(encryptionKey)

//         store.add(Login(
//                 id = "aaaaaaaaaaaa",
//                 hostname = "https://www.example.com",
//                 httpRealm = "Something",
//                 username = "Foobar2000",
//                 password = "hunter2",
//                 usernameField = "users_name",
//                 passwordField = "users_password",
//                 formSubmitUrl = null,
//                 timesUsed = 0,
//                 timeCreated = 0,
//                 timeLastUsed = 0,
//                 timePasswordChanged = 0
//         ))

//         store.add(Login(
//                 id = "bbbbbbbbbbbb",
//                 username = "Foobar2000",
//                 hostname = "https://www.example.org",
//                 httpRealm = "",
//                 formSubmitUrl = "https://www.example.org/login",
//                 password = "MyVeryCoolPassword",
//                 usernameField = "users_name",
//                 passwordField = "users_password",
//                 timesUsed = 0,
//                 timeCreated = 0,
//                 timeLastUsed = 0,
//                 timePasswordChanged = 0
//         ))

//         store.lock()

//         return store
//     }

//     protected fun finishAndClose(store: DatabaseLoginsStorage) {
//         store.ensureLocked()
//         assertEquals(store.isLocked(), true)
//         store.close()
//     }

//     @Test
//     fun testMetricsGathering() {
//         val store = createTestStore()
//         val key = "0123456789abcdef"

//         assert(!LoginsStoreMetrics.unlockCount.testHasValue())
//         assert(!LoginsStoreMetrics.unlockErrorCount["invalid_key"].testHasValue())

//         store.unlock(key)

//         assertEquals(LoginsStoreMetrics.unlockCount.testGetValue(), 1)
//         assert(!LoginsStoreMetrics.unlockErrorCount["invalid_key"].testHasValue())

//         store.lock()
//         try {
//             store.unlock("wrongkey")
//             fail("Should have thrown")
//         } catch (e: LoginsStorageErrorException.InvalidKey) {
//             // All good.
//         }
//         store.unlock(key)

//         assertEquals(LoginsStoreMetrics.unlockCount.testGetValue(), 3)
//         assert(LoginsStoreMetrics.unlockErrorCount["invalid_key"].testHasValue())
//         assertEquals(LoginsStoreMetrics.unlockErrorCount["invalid_key"].testGetValue(), 1)

//         try {
//             store.unlock(key)
//             fail("Should have thrown")
//         } catch (e: LoginsStorageErrorException.MismatchedLock) {
//             // All good.
//         }
//         assertEquals(LoginsStoreMetrics.unlockCount.testGetValue(), 4)
//         assert(LoginsStoreMetrics.unlockErrorCount["mismatched_lock"].testHasValue())
//         assertEquals(LoginsStoreMetrics.unlockErrorCount["mismatched_lock"].testGetValue(), 1)

//         assert(!LoginsStoreMetrics.writeQueryCount.testHasValue())
//         assert(!LoginsStoreMetrics.writeQueryErrorCount["invalid_record"].testHasValue())

//         store.add(Login(
//                 id = "aaaaaaaaaaaa",
//                 hostname = "https://www.example.com",
//                 httpRealm = "Something",
//                 username = "Foobar2000",
//                 password = "hunter2",
//                 usernameField = "users_name",
//                 passwordField = "users_password",
//                 formSubmitUrl = null,
//                 timesUsed = 0,
//                 timeCreated = 0,
//                 timeLastUsed = 0,
//                 timePasswordChanged = 0
//         ))

//         assertEquals(LoginsStoreMetrics.writeQueryCount.testGetValue(), 1)
//         assert(!LoginsStoreMetrics.writeQueryErrorCount["invalid_record"].testHasValue())

//         // N.B. this is invalid due to `formSubmitURL` being an invalid url.
//         val invalid = Login(
//             id = "bbbbbbbbbbbb",
//             hostname = "https://test.example.com",
//             formSubmitUrl = "not a url",
//             httpRealm = "",
//             username = "Foobar2000",
//             password = "hunter2",
//             usernameField = "users_name",
//             passwordField = "users_password",
//             timesUsed = 0,
//             timeCreated = 0,
//             timeLastUsed = 0,
//             timePasswordChanged = 0
//         )

//         try {
//             store.add(invalid)
//             fail("Should have thrown")
//         } catch (e: LoginsStorageErrorException.InvalidRecord) {
//             // All good.
//         }

//         assertEquals(LoginsStoreMetrics.writeQueryCount.testGetValue(), 2)
//         assertEquals(LoginsStoreMetrics.writeQueryErrorCount["invalid_record"].testGetValue(), 1)

//         assert(!LoginsStoreMetrics.readQueryCount.testHasValue())
//         assert(!LoginsStoreMetrics.readQueryErrorCount["storage_error"].testHasValue())

//         val record = store.get("aaaaaaaaaaaa")!!
//         assertEquals(record.hostname, "https://www.example.com")

//         assertEquals(LoginsStoreMetrics.readQueryCount.testGetValue(), 1)
//         assert(!LoginsStoreMetrics.readQueryErrorCount["storage_error"].testHasValue())

//         // Ensure that ensureValid doesn't cause us to record invalid_record errors.
//         try {
//             store.ensureValid(invalid)
//             fail("Should have thrown")
//         } catch (e: LoginsStorageErrorException.InvalidRecord) {
//             // All good.
//         }

//         assertEquals(LoginsStoreMetrics.readQueryCount.testGetValue(), 2)
//         assert(!LoginsStoreMetrics.readQueryErrorCount["invalid_record"].testHasValue())

//         finishAndClose(store)
//     }

//     @Test
//     fun testLockedOperations() {
//         val test = getTestStore()
//         assertEquals(test.isLocked(), true)

//         assertThrows(LoginsStorageErrorException::class.java) { test.get("aaaaaaaaaaaa") }
//         assertThrows(LoginsStorageErrorException::class.java) { test.list() }
//         assertThrows(LoginsStorageErrorException::class.java) { test.delete("aaaaaaaaaaaa") }
//         assertThrows(LoginsStorageErrorException::class.java) { test.touch("bbbbbbbbbbbb") }
//         assertThrows(LoginsStorageErrorException::class.java) { test.wipe() }
//         assertThrows(LoginsStorageErrorException::class.java) {
//             @Suppress("DEPRECATION")
//             test.reset()
//         }

//         test.unlock(encryptionKey)
//         assertEquals(test.isLocked(), false)
//         // Make sure things didn't change despite being locked
//         assertNotNull(test.get("aaaaaaaaaaaa"))
//         // "bbbbbbbbbbbb" has a single use (from insertion)
//         assertEquals(1, test.get("bbbbbbbbbbbb")!!.timesUsed)
//         finishAndClose(test)
//     }

//     @Test
//     fun testEnsureLockUnlock() {
//         val test = getTestStore()
//         assertEquals(test.isLocked(), true)

//         test.ensureUnlocked(encryptionKey)
//         assertEquals(test.isLocked(), false)
//         test.ensureUnlocked(encryptionKey)
//         assertEquals(test.isLocked(), false)

//         test.ensureLocked()
//         assertEquals(test.isLocked(), true)
//         test.ensureLocked()
//         assertEquals(test.isLocked(), true)

//         finishAndClose(test)
//     }

//     @Test
//     fun testTouch() {
//         val test = getTestStore()
//         test.unlock(encryptionKey)
//         assertEquals(test.list().size, 2)
//         val b = test.get("bbbbbbbbbbbb")!!

//         // Wait 100ms so that touch is certain to change timeLastUsed.
//         Thread.sleep(100)
//         test.touch("bbbbbbbbbbbb")

//         val newB = test.get("bbbbbbbbbbbb")

//         assertNotNull(newB)
//         assertEquals(b.timesUsed + 1, newB!!.timesUsed)
//         assert(newB.timeLastUsed > b.timeLastUsed)

//         assertThrows(LoginsStorageErrorException.NoSuchRecord::class.java) { test.touch("abcdabcdabcd") }

//         finishAndClose(test)
//     }

//     @Test
//     fun testDelete() {
//         val test = getTestStore()

//         test.unlock(encryptionKey)
//         assertNotNull(test.get("aaaaaaaaaaaa"))
//         assertTrue(test.delete("aaaaaaaaaaaa"))
//         assertNull(test.get("aaaaaaaaaaaa"))
//         assertFalse(test.delete("aaaaaaaaaaaa"))
//         assertNull(test.get("aaaaaaaaaaaa"))

//         finishAndClose(test)
//     }

//     @Test
//     fun testListWipe() {
//         val test = getTestStore()
//         test.unlock(encryptionKey)
//         assertEquals(2, test.list().size)

//         test.wipe()
//         assertEquals(0, test.list().size)

//         assertNull(test.get("aaaaaaaaaaaa"))
//         assertNull(test.get("bbbbbbbbbbbb"))

//         finishAndClose(test)
//     }

//     @Test
//     fun testWipeLocal() {
//         val test = getTestStore()
//         test.unlock(encryptionKey)
//         assertEquals(2, test.list().size)

//         test.wipeLocal()
//         assertEquals(0, test.list().size)

//         assertNull(test.get("aaaaaaaaaaaa"))
//         assertNull(test.get("bbbbbbbbbbbb"))

//         finishAndClose(test)
//     }

//     @Test

//     fun testAdd() {
//         val test = getTestStore()
//         test.unlock(encryptionKey)

//         assertThrows(LoginsStorageErrorException.IdCollision::class.java) {
//             test.add(Login(
//                     id = "aaaaaaaaaaaa",
//                     hostname = "https://www.foo.org",
//                     httpRealm = "Some Realm",
//                     password = "MyPassword",
//                     username = "MyUsername",
//                     usernameField = "",
//                     passwordField = "",
//                     formSubmitUrl = "",
//                     timesUsed = 0,
//                     timeCreated = 0,
//                     timeLastUsed = 0,
//                     timePasswordChanged = 0
//             ))
//         }

//         for (record in INVALID_RECORDS) {
//             assertThrows(LoginsStorageErrorException.InvalidRecord::class.java) {
//                 test.add(record)
//             }
//         }

//         val toInsert = Login(
//                 id = "",
//                 hostname = "https://www.foo.org",
//                 httpRealm = "Some Realm",
//                 password = "MyPassword",
//                 username = "Foobar2000",
//                 usernameField = "",
//                 passwordField = "",
//                 formSubmitUrl = null,
//                 timesUsed = 0,
//                 timeCreated = 0,
//                 timeLastUsed = 0,
//                 timePasswordChanged = 0
//         )

//         val generatedID = test.add(toInsert)

//         val record = test.get(generatedID)!!
//         assertEquals(generatedID, record.id)
//         assertEquals(toInsert.hostname, record.hostname)
//         assertEquals(toInsert.httpRealm, record.httpRealm)
//         assertEquals(toInsert.password, record.password)
//         assertEquals(toInsert.username, record.username)
//         assertEquals(toInsert.passwordField, record.passwordField)
//         assertEquals(toInsert.usernameField, record.usernameField)
//         assertEquals(toInsert.formSubmitUrl, record.formSubmitUrl)
//         assertEquals(1, record.timesUsed)

//         assertNotEquals(0L, record.timeLastUsed)
//         assertNotEquals(0L, record.timeCreated)
//         assertNotEquals(0L, record.timePasswordChanged)

//         val specificID = test.add(Login(
//                 id = "123412341234",
//                 hostname = "http://www.bar.com",
//                 formSubmitUrl = "http://login.bar.com",
//                 password = "DummyPassword",
//                 username = "DummyUsername",
//                 usernameField = "users_name",
//                 passwordField = "users_password",
//                 httpRealm = null,
//                 timesUsed = 0,
//                 timeCreated = 0,
//                 timeLastUsed = 0,
//                 timePasswordChanged = 0
//         ))

//         assertEquals("123412341234", specificID)

//         finishAndClose(test)
//     }

//     @Test
//     fun testEnsureValid() {
//         val test = getTestStore()
//         test.unlock(encryptionKey)

//         test.add(Login(
//                 id = "bbbbb",
//                 hostname = "https://www.foo.org",
//                 httpRealm = "Some Realm",
//                 password = "MyPassword",
//                 username = "MyUsername",
//                 usernameField = "",
//                 passwordField = "",
//                 formSubmitUrl = null,
//                 timesUsed = 0,
//                 timeCreated = 0,
//                 timeLastUsed = 0,
//                 timePasswordChanged = 0
//         ))

//         val dupeLogin = Login(
//                 id = "",
//                 hostname = "https://www.foo.org",
//                 httpRealm = "Some Realm",
//                 password = "MyPassword",
//                 username = "MyUsername",
//                 usernameField = "",
//                 passwordField = "",
//                 formSubmitUrl = "",
//                 timesUsed = 0,
//                 timeCreated = 0,
//                 timeLastUsed = 0,
//                 timePasswordChanged = 0
//         )

//         val nullValueLogin = Login(
//                 id = "",
//                 hostname = "https://www.test.org",
//                 httpRealm = "Some Other Realm",
//                 password = "MyPassword",
//                 username = "\u0000MyUsername2",
//                 usernameField = "",
//                 passwordField = "",
//                 formSubmitUrl = "",
//                 timesUsed = 0,
//                 timeCreated = 0,
//                 timeLastUsed = 0,
//                 timePasswordChanged = 0
//         )

//         assertThrows(LoginsStorageErrorException.InvalidRecord::class.java) {
//             test.ensureValid(dupeLogin)
//         }

//         assertThrows(LoginsStorageErrorException.InvalidRecord::class.java) {
//             test.ensureValid(nullValueLogin)
//         }

//         test.delete("bbbbb")
//     }

//     @Test
//     fun testPotentialDupesIgnoringUsername() {
//         val test = getTestStore()
//         test.unlock(encryptionKey)

//         val savedLogin1 = Login(
//                 id = "bbbbb",
//                 hostname = "https://www.foo.org",
//                 httpRealm = "Some Realm",
//                 password = "MyPassword",
//                 username = "MyUsername",
//                 usernameField = "",
//                 passwordField = "",
//                                 formSubmitUrl = "",
//                 timesUsed = 0,
//                 timeCreated = 0,
//                 timeLastUsed = 0,
//                 timePasswordChanged = 0
//         )

//         test.add(savedLogin1)

//         val dupeLogin = Login(
//                 id = "",
//                 hostname = "https://www.foo.org",
//                 httpRealm = "Some Realm",
//                 password = "MyPassword",
//                 username = "MySecondUsername",
//                 usernameField = "",
//                 passwordField = "",
//                                 formSubmitUrl = "",
//                 timesUsed = 0,
//                 timeCreated = 0,
//                 timeLastUsed = 0,
//                 timePasswordChanged = 0
//         )

//         val potentialDupes = test.potentialDupesIgnoringUsername(dupeLogin)
//         assert(potentialDupes.size == 1)
//         assertEquals(potentialDupes[0].id, savedLogin1.id)

//         test.delete("bbbbb")
//     }

//     @Test
//     fun testUpdate() {
//         val test = getTestStore()
//         test.unlock(encryptionKey)

//         assertThrows(LoginsStorageErrorException.NoSuchRecord::class.java) {
//             test.update(Login(
//                     id = "123412341234",
//                     hostname = "https://www.foo.org",
//                     httpRealm = "Some Realm",
//                     password = "MyPassword",
//                     username = "MyUsername",
//                     usernameField = "",
//                     passwordField = "",
//                     formSubmitUrl = "",
//                         timesUsed = 0,
//                         timeCreated = 0,
//                         timeLastUsed = 0,
//                         timePasswordChanged = 0
//             ))
//         }

//         for (record in INVALID_RECORDS) {
//             val updateArg = record.copy(id = "aaaaaaaaaaaa")
//             assertThrows(LoginsStorageErrorException.InvalidRecord::class.java) {
//                 test.update(updateArg)
//             }
//         }

//         val toUpdate = test.get("aaaaaaaaaaaa")!!.copy(
//                 password = "myNewPassword"
//         )

//         // Sleep so that the current time for test.update is guaranteed to be
//         // different.
//         Thread.sleep(100)

//         test.update(toUpdate)

//         val record = test.get(toUpdate.id)!!
//         assertEquals(toUpdate.hostname, record.hostname)
//         assertEquals(toUpdate.httpRealm, record.httpRealm)
//         assertEquals(toUpdate.password, record.password)
//         assertEquals(toUpdate.username, record.username)
//         assertEquals(toUpdate.passwordField, record.passwordField)
//         assertEquals(toUpdate.usernameField, record.usernameField)
//         assertEquals(toUpdate.formSubmitUrl, record.formSubmitUrl)
//         assertEquals(toUpdate.timesUsed + 1, record.timesUsed)
//         assertEquals(toUpdate.timeCreated, record.timeCreated)

//         assert(toUpdate.timeLastUsed < record.timeLastUsed)

//         assert(toUpdate.timeLastUsed < record.timeLastUsed)
//         assert(toUpdate.timeLastUsed < record.timePasswordChanged)

//         val specificID = test.add(Login(
//                 id = "123412341234",
//                 hostname = "http://www.bar.com",
//                 formSubmitUrl = "http://login.bar.com",
//                 httpRealm = "",
//                 password = "DummyPassword",
//                 username = "DummyUsername",
//                 usernameField = "users_name",
//                 passwordField = "users_password",
//                 timesUsed = 0,
//                 timeCreated = 0,
//                 timeLastUsed = 0,
//                 timePasswordChanged = 0
//         ))

//         assertEquals("123412341234", specificID)

//         finishAndClose(test)
//     }

//     @Test
//     @Suppress("DEPRECATION")
//     fun testUnlockAfterError() {
//         val test = getTestStore()

//         assertThrows(LoginsStorageErrorException::class.java) {
//             test.reset()
//         }

//         test.unlock(encryptionKey)

//         test.reset()

//         finishAndClose(test)
//     }

//     companion object {
//         val INVALID_RECORDS: List<Login> = listOf(
//                 // Invalid formSubmitUrl
//                 Login(
//                         id = "",
//                         hostname = "https://www.foo.org",
//                         httpRealm = null,
//                         formSubmitUrl = "invalid\u0000value",
//                         password = "MyPassword",
//                         username = "MyUsername",
//                         usernameField = "users_name",
//                         passwordField = "users_password",
//                         timesUsed = 0,
//                         timeCreated = 0,
//                         timeLastUsed = 0,
//                         timePasswordChanged = 0
//                 ),
//                 // Neither formSubmitUrl nor httpRealm
//                 Login(
//                         id = "",
//                         hostname = "https://www.foo.org",
//                         httpRealm = null,
//                         password = "MyPassword",
//                         username = "MyUsername",
//                         usernameField = "",
//                         passwordField = "",
//                         formSubmitUrl = null,
//                         timesUsed = 0,
//                         timeCreated = 0,
//                         timeLastUsed = 0,
//                         timePasswordChanged = 0
//                 ),
//                 // Empty password
//                 Login(
//                         id = "",
//                         hostname = "https://www.foo.org",
//                         httpRealm = "Some Realm",
//                         password = "",
//                         username = "MyUsername",
//                         usernameField = "",
//                         passwordField = "",
//                         formSubmitUrl = null,
//                         timesUsed = 0,
//                         timeCreated = 0,
//                         timeLastUsed = 0,
//                         timePasswordChanged = 0
//                 ),
//                 // Empty hostname
//                 Login(
//                         id = "",
//                         hostname = "",
//                         httpRealm = "Some Realm",
//                         password = "MyPassword",
//                         username = "MyUsername",
//                         usernameField = "",
//                         passwordField = "",
//                         formSubmitUrl = null,
//                         timesUsed = 0,
//                         timeCreated = 0,
//                         timeLastUsed = 0,
//                         timePasswordChanged = 0
//                 )
//         )
//     }
// }
