/* Any copyright is dedicated to the Public Domain.
   http://creativecommons.org/publicdomain/zero/1.0/ */

package mozilla.appservices.rustlog

import junit.framework.Assert
import org.junit.AfterClass
import org.junit.BeforeClass
import org.junit.rules.TemporaryFolder
import org.junit.Rule
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

import org.junit.Test
import org.junit.Assert.*
import java.lang.RuntimeException

@RunWith(RobolectricTestRunner::class)
@Config(manifest = Config.NONE)
class LogTest {

    fun writeTestLog(m: String) {
        LibRustLogAdapter.INSTANCE.ac_log_adapter_test__log_msg(m)
        Thread.sleep(100) // Wait for it to arrive...
    }
    // This test is big and monolithic, mostly because we can't re-enable the log system
    // after shutting it down. Ugh.
    @Test
    fun testLogging() {
        val logs: MutableList<String> = mutableListOf()

        assert(!RustLogAdapter.isEnabled)
        assert(RustLogAdapter.canEnable)

        RustLogAdapter.enable { level, tagStr, msgStr ->
            val threadId = Thread.currentThread().id
            val info = "Rust log from $threadId | Level: $level | tag: $tagStr | message: $msgStr"
            println(info)
            logs += info
            true
        }

        // We log an informational message after initializing (but it's processed asynchronously).
        Thread.sleep(100)
        assertEquals(logs.size, 1)

        writeTestLog("Test1")

        assertEquals(logs.size, 2)

        assert(RustLogAdapter.isEnabled)
        assert(!RustLogAdapter.canEnable)
        var wasCalled = false;

        val didEnable = RustLogAdapter.tryEnable { _, _, _ ->
            wasCalled = true
            true
        }

        assert(!didEnable);
        writeTestLog("Test2")

        assertEquals(logs.size, 3)
        assert(!wasCalled)

        // Adjust the max level so that the test log (which is logged at info level)
        // will not be present.
        RustLogAdapter.setMaxLevel(LogLevelFilter.WARN)

        writeTestLog("Test3")

        assertEquals(logs.size, 3)


        // Make sure we can re-enable it
        RustLogAdapter.setMaxLevel(LogLevelFilter.INFO)
        writeTestLog("Test4")

        assertEquals(logs.size, 4)


        RustLogAdapter.disable()
        assert(!RustLogAdapter.isEnabled)
        assert(!RustLogAdapter.canEnable)

        // Shouldn't do anything, we disabled the log.
        writeTestLog("Test5")

        assertEquals(logs.size, 4)
        assert(!wasCalled)

        val didEnable2 = RustLogAdapter.tryEnable { _, _, _ ->
            wasCalled = true
            true
        }
        assert(!didEnable2)

        try {
            RustLogAdapter.enable { _, _, _ ->
                wasCalled = true
                true
            }
            Assert.fail("enable should throw")
        } catch (e: LogAdapterCannotEnable) {
        }

        // One last time to make sure that those enable/tryEnable
        // calls didn't secretly work.
        writeTestLog("Test6")
        assert(!wasCalled)
        // XXX FIXME work out how we can test returning false!
    }
}

