/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.experiments.nimbus

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.mozilla.experiments.nimbus.util.TestNimbusBuilder
import org.robolectric.RobolectricTestRunner
import java.util.concurrent.TimeUnit

@RunWith(RobolectricTestRunner::class)
class EventStoreTest {
    private val context: Context
        get() = ApplicationProvider.getApplicationContext()

    private val appInfo = NimbusAppInfo(
        appName = "NimbusUnitTest",
        channel = "test",
    )

    private val nimbus = TestNimbusBuilder(context).build(appInfo)

    val events: NimbusEventStore
        get() = nimbus.events

    val eventId = "app_launched"

    fun createHelper() = nimbus.createMessageHelper()

    @Test
    fun `recording events in the past`() {
        val helper = createHelper()
        events.recordPastEvent(1, eventId, 24, TimeUnit.HOURS)

        assertTrue(helper.evalJexl("'$eventId'|eventLastSeen('Days') == 1"))
        assertTrue(helper.evalJexl("'$eventId'|eventLastSeen('Hours') == 24"))
    }

    @Test
    fun `advancing time into the future`() {
        val helper = createHelper()
        events.recordEvent(eventId)

        assertTrue(helper.evalJexl("'$eventId'|eventLastSeen('Days') == 0"))

        events.advanceEventTime(24, TimeUnit.HOURS)

        assertTrue(helper.evalJexl("'$eventId'|eventLastSeen('Days') == 1"))
        assertTrue(helper.evalJexl("'$eventId'|eventLastSeen('Hours') == 24"))

        events.advanceEventTime(24, TimeUnit.HOURS)
        assertTrue(helper.evalJexl("'$eventId'|eventLastSeen('Days') == 2"))
        assertTrue(helper.evalJexl("'$eventId'|eventLastSeen('Hours') == 48"))
    }
}
