/* Any copyright is dedicated to the Public Domain.
   http://creativecommons.org/publicdomain/zero/1.0/ */

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
