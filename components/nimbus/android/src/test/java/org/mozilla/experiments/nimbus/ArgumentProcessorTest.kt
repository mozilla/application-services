/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.experiments.nimbus

import android.net.Uri
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import java.net.URLEncoder

@RunWith(RobolectricTestRunner::class)
class ArgumentProcessorTest {
    fun `test createCliArgsFromUri flags`() {
        val obs = createCommandLineArgs(
            Uri.parse("my-app://foo?--nimbus-cli"),
        )
        assertNotNull(obs)
        assertEquals(CliArgs(false, null, false), obs)

        val obs1 = createCommandLineArgs(
            Uri.parse("my-app://foo?--nimbus-cli&--reset-db"),
        )
        assertEquals(CliArgs(true, null, false), obs1)

        val obs2 = createCommandLineArgs(
            Uri.parse("my-app://foo?--reset-db&--nimbus-cli&--log-state"),
        )
        assertEquals(CliArgs(true, null, true), obs2)

        val obs3 = createCommandLineArgs(
            Uri.parse("my-app://foo?--nimbus-cli=true&--reset-db=1&--log-state"),
        )
        assertEquals(CliArgs(true, null, true), obs3)

        val obs4 = createCommandLineArgs(
            Uri.parse("my-app://foo?--nimbus-cli&--reset-db=0&--log-state=false"),
        )
        assertEquals(CliArgs(false, null, false), obs4)
    }

    @Test
    fun `test createCliArgsFromUri experiments`() {
        val unenrollAll = "{\"data\":[]}"
        val obs = createCommandLineArgs(
            Uri.parse("my-app://foo?--nimbus-cli&--experiments=$unenrollAll"),
        )
        assertNotNull(obs)
        assertEquals(CliArgs(false, unenrollAll, false), obs)

        val encoded = URLEncoder.encode(unenrollAll, "UTF-8")
        assertNotEquals(encoded, unenrollAll)

        val obs1 = createCommandLineArgs(
            Uri.parse("my-app://foo?--nimbus-cli&--experiments=$encoded"),
        )
        assertNotNull(obs1)
        assertEquals(CliArgs(false, unenrollAll, false), obs1)
    }

    @Test
    fun `test createCliArgsFromUri experiments sanity check`() {
        val good = "{\"data\":[]}"
        val obs = createCommandLineArgs(
            Uri.parse("my-app://foo?--nimbus-cli&--experiments=$good"),
        )
        assertNotNull(obs)
        assertEquals(CliArgs(false, good, false), obs)

        fun isInvalid(bad: String) {
            val obs0 = createCommandLineArgs(
                Uri.parse("my-app://foo?--nimbus-cli&--experiments=$bad"),
            )
            assertNull(obs0)
        }

        isInvalid("{}")
        isInvalid("[]")
        isInvalid("{\"data\": 1}")
    }
}
