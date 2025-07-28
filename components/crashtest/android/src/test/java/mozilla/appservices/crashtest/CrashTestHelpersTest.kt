/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.crashtest

import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(manifest = Config.NONE)
class CrashTestHelpersTest {
    @Test
    fun testPanicsAreCaughtAndThrown() {
        try {
            triggerRustPanic()
        } catch (e: InternalException) {
            assertEquals(e.message, "Panic! In The Rust Code.")
        }
    }

    @Test
    fun testErrorsAreThrown() {
        try {
            triggerRustError()
        } catch (e: CrashTestException) {
            assertEquals(e.message, "Error! From The Rust Code.")
        }
    }

    // We can't test `triggerRustAbort()` here because it's a hard crash.
}
