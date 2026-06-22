/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.experiments.nimbus

import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.mozilla.experiments.nimbus.internal.FeatureUpdateDispatcher
import org.robolectric.RobolectricTestRunner

@RunWith(RobolectricTestRunner::class)
class FeatureUpdateDispatcherTests {
    @Test
    fun `test update registration`() {
        val updates = FeatureUpdateDispatcher()

        var fooCalls = 0
        var barCalls = 0
        var bazCalls = 0

        fun assertCalls(expectedFoo: Int, expectedBar: Int, expectedBaz: Int) {
            assertEquals(expectedFoo, fooCalls)
            assertEquals(expectedBar, barCalls)
            assertEquals(expectedBaz, bazCalls)
        }

        val fooCallback: () -> Unit = { fooCalls++ }
        val barCallback: () -> Unit = { barCalls++ }
        val bazCallback: () -> Unit = { bazCalls++ }

        assertCalls(0, 0, 0)

        updates.notifyFeatures(setOf("foo", "bar", "baz"))
        assertCalls(0, 0, 0)

        updates.addCallback("foo", fooCallback)
        updates.notifyFeatures(setOf("foo", "bar", "baz"))
        assertCalls(1, 0, 0)

        updates.addCallback("bar", barCallback)
        updates.notifyFeatures(setOf("foo", "bar", "baz"))
        assertCalls(2, 1, 0)

        updates.addCallback("baz", bazCallback)
        updates.notifyFeatures(setOf("foo", "bar", "baz"))
        assertCalls(3, 2, 1)

        updates.removeCallback("foo", fooCallback)
        updates.notifyFeatures(setOf("foo", "bar", "baz"))
        assertCalls(3, 3, 2)

        updates.removeCallback("bar", barCallback)
        updates.notifyFeatures(setOf("foo", "bar", "baz"))
        assertCalls(3, 3, 3)

        updates.removeCallback("baz", bazCallback)
        updates.notifyFeatures(setOf("foo", "bar", "baz"))
        assertCalls(3, 3, 3)
    }

    @Test
    fun `multiple callbacks for feature`() {
        val updates = FeatureUpdateDispatcher()

        var aCalls = 0
        var bCalls = 0

        fun assertCalls(expectedA: Int, expectedB: Int) {
            assertEquals(expectedA, aCalls)
            assertEquals(expectedB, bCalls)
        }

        val callbackA: () -> Unit = { aCalls++ }
        val callbackB: () -> Unit = { bCalls++ }

        assertCalls(0, 0)

        updates.notifyFeatures(setOf("foo"))
        assertCalls(0, 0)

        updates.addCallback("foo", callbackA)
        updates.notifyFeatures(setOf("foo"))
        assertCalls(1, 0)

        updates.addCallback("foo", callbackB)
        updates.notifyFeatures(setOf("foo"))
        assertCalls(2, 1)

        updates.removeCallback("foo", callbackA)
        updates.notifyFeatures(setOf("foo"))
        assertCalls(2, 2)

        updates.removeCallback("foo", callbackB)
        updates.notifyFeatures(setOf("foo"))
        assertCalls(2, 2)
    }
}
