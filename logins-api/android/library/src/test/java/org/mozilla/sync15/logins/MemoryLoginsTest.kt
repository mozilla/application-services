/* Copyright 2018 Mozilla
 * Licensed under the Apache License, Version 2.0 (the "License"); you may not use
 * this file except in compliance with the License. You may obtain a copy of the
 * License at http://www.apache.org/licenses/LICENSE-2.0
 * Unless required by applicable law or agreed to in writing, software distributed
 * under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
 * CONDITIONS OF ANY KIND, either express or implied. See the License for the
 * specific language governing permissions and limitations under the License. */
package org.mozilla.sync15.logins

import org.junit.Assert
import org.junit.Test
import org.junit.Assert.*
import java.util.*
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Semaphore
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

class MemoryLoginsTest {

    fun getTestStore(): LoginsStorage {
        return MemoryLoginsStorage(listOf(
                ServerPassword(
                        id = "aaaaaaaaaaaa",
                        hostname = "https://www.example.com",
                        httpRealm = "Something",
                        formSubmitURL = null,
                        username = "Foobar2000",
                        password = "hunter2",
                        timesUsed = 3,
                        timeLastUsed = Date(2018, 7, 7).time,
                        timeCreated = Date(2018, 7, 5).time,
                        timePasswordChanged = Date(2018, 7, 6).time,
                        usernameField = "users_name",
                        passwordField = "users_password"
                ),
                ServerPassword(
                        id = "bbbbbbbbbbbb",
                        hostname = "https://www.example.org",
                        httpRealm = null,
                        formSubmitURL = "https://www.example.org/login",
                        password = "MyVeryCoolPassword",
                        timesUsed = 0,
                        timeLastUsed = 0L,
                        timeCreated = 0L,
                        timePasswordChanged = 0L,
                        username = null,
                        usernameField = null,
                        passwordField = null
                )
        ))
    }
    // TODO This should return T and not T?, I can't figure out how to coerce a T? to a T that
    // doesn't throw if T is really nullable (e.g. `waitForResult<Int?>(SyncResult.fromValue(null))`
    // should be fine!).
    fun <T> waitForResult(e: SyncResult<T>): T? {
        val result: AtomicReference<T?> = AtomicReference(null);
        val finished = AtomicBoolean(false) // Needed since T may be nullable
        val gate = CountDownLatch(1)
        e.thenCatch { err ->
            fail(err.message)

            throw err // Needed to typecheck, kotlin doesn't know that fail() doesn't return
        }.whenComplete {
            result.set(it)
            if (finished.getAndSet(true)) {
                fail("Timed out!");
            }
            gate.countDown()
        }
        // This is clumsy... If this takes more than 2s we assume we're hosed.
        val waiter = object: Thread() {
            override fun run() {
                Thread.sleep(2000)
                if (result.get() == null) {
                    finished.set(true)
                    gate.countDown()
                }
            }
        }
        waiter.start()
        gate.await()
        val v = result.get();
        assert(finished.get())
        return v
    }

    fun <T> waitForException(e: SyncResult<T>): Exception {
        val result: AtomicReference<Exception?> = AtomicReference(null);
        val gate = CountDownLatch(1)
        e.then({
            fail("Expected exception but resolved successfully");
            SyncResult.fromValue(Unit)
        }) { err ->
            if (result.getAndSet(err) != null) {
                // Probably something will have killed this thread by this point.
                fail("Timed Out!");
            }
            gate.countDown();
            SyncResult.fromValue(Unit)
        }
        // This is clumsy... If this takes more than 2s we assume we're hosed.
        val waiter = object: Thread() {
            override fun run() {
                Thread.sleep(2000)
                if (result.get() == null) {
                    gate.countDown()
                }
            }
        }
        waiter.start()

        gate.await()
        val v = result.get();
        assertNotNull(v);
        return v!!
    }

    fun finishAndClose(store: LoginsStorage) {
        waitForResult(store.lock())
        assertEquals(waitForResult(store.isLocked()), true);
        store.close() // Good habit, and avoids issues if we ever actually use mentat in this code or something
    }

    @Test
    fun testLockedOperations() {
        val test = getTestStore();
        assertEquals(waitForResult(test.isLocked()), true);
        // Note that waitForException fails the test if it successfully resolves
        waitForException(test.get("aaaaaaaaaaaa"))
        waitForException(test.list())
        waitForException(test.delete("aaaaaaaaaaaa"))
        waitForException(test.touch("bbbbbbbbbbbb"))
        waitForException(test.wipe())
        waitForException(test.sync(SyncUnlockInfo("", "", "", "")))
        waitForException(test.reset())

        waitForResult(test.unlock(""))
        assertEquals(waitForResult(test.isLocked()), false);
        // Make sure things didn't change despite being locked
        assertNotNull(waitForResult(test.get("aaaaaaaaaaaa")))
        // "bbbbbbbbbbbb" Starts without ever having been touched.
        assertEquals(0, waitForResult(test.get("bbbbbbbbbbbb"))!!.timesUsed)
        finishAndClose(test)
    }

    @Test
    fun testTouch() {
        val test = getTestStore()
        waitForResult(test.unlock(""))
        assertEquals(waitForResult(test.list())!!.size, 2)
        val b = waitForResult(test.get("bbbbbbbbbbbb"))!!
        assertEquals(0, b.timesUsed)
        assertEquals(0L, b.timeLastUsed)
        waitForResult(test.touch("bbbbbbbbbbbb"))

        assertEquals(b.timesUsed, 0) // Shouldn't change previously returned object
        assertEquals(b.timeLastUsed, 0L)

        val newB = waitForResult(test.get("bbbbbbbbbbbb"))

        assertNotNull(newB)
        assertEquals(1, newB!!.timesUsed)
        assert(newB.timeLastUsed > 0L)

        finishAndClose(test)
    }

    @Test
    fun testDelete() {
        val test = getTestStore();
        waitForResult(test.unlock(""))

        assertNotNull(waitForResult(test.get("aaaaaaaaaaaa")))
        assertTrue(waitForResult(test.delete("aaaaaaaaaaaa"))!!)
        assertNull(waitForResult(test.get("aaaaaaaaaaaa")))
        assertFalse(waitForResult(test.delete("aaaaaaaaaaaa"))!!)

        assertNull(waitForResult(test.get("aaaaaaaaaaaa")))

        finishAndClose(test)
    }

    @Test
    fun testListWipe() {
        val test = getTestStore();
        waitForResult(test.unlock(""))
        assertEquals(2, waitForResult(test.list())!!.size);

        waitForResult(test.wipe())
        assertEquals(0, waitForResult(test.list())!!.size);

        assertNull(waitForResult(test.get("aaaaaaaaaaaa")))
        assertNull(waitForResult(test.get("bbbbbbbbbbbb")))

        finishAndClose(test)
    }
}