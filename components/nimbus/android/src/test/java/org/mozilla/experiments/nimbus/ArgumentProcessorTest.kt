/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.experiments.nimbus

import android.content.Intent
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
    @Test
    fun `test createCommandLineArgs`() {
        // The intent must set the nimbus-cli and version extras in order to be processed.
        assertNull(createCommandLineArgs(Intent("bogus")))
        assertNull(
            createCommandLineArgs(Intent("bogus").apply { putExtra("nimbus-cli", null as String?) }),
        )
        assertNull(
            createCommandLineArgs(Intent("bogus").apply { putExtra("version", 1) }),
        )
        assertEquals(
            createCommandLineArgs(createIntent()),
            CliArgs(),
        )

        // reset-db is a boolean key
        assertEquals(
            CliArgs(),
            createCommandLineArgs(createIntent().apply { putExtra("reset-db", null as String?) }),
        )

        assertEquals(
            CliArgs(),
            createCommandLineArgs(createIntent().apply { putExtra("reset-db", 1) }),
        )

        assertEquals(
            CliArgs(resetDatabase = true),
            createCommandLineArgs(createIntent().apply { putExtra("reset-db", true) }),
        )

        // log-state is a boolean key
        assertEquals(
            CliArgs(),
            createCommandLineArgs(createIntent().apply { putExtra("log-state", null as String?) }),
        )

        assertEquals(
            CliArgs(),
            createCommandLineArgs(createIntent().apply { putExtra("log-state", 1) }),
        )

        assertEquals(
            CliArgs(logState = true),
            createCommandLineArgs(createIntent().apply { putExtra("log-state", true) }),
        )
    }

    @Test
    fun `test createCommandLineArgs with experiments`() {
        val empty = """{"data": []}"""
        assertEquals(
            CliArgs(
                experiments = empty,
            ),
            createCommandLineArgs(createIntent().apply { putExtra("experiments", empty) }),
        )

        // &apos; is escaped.
        val withApos = """{"data": [{"slug": "&apos;"}]}"""
        val withQuot = """{"data": [{"slug": "'"}]}"""
        assertEquals(
            CliArgs(experiments = withQuot),
            createCommandLineArgs(createIntent().apply { putExtra("experiments", withApos) }),
        )

        assertNull(createCommandLineArgs(createIntent().apply { putExtra("experiments", "{}") }))
        assertNull(createCommandLineArgs(createIntent().apply { putExtra("experiments", "[]") }))
        assertNull(
            createCommandLineArgs(
                createIntent().apply {
                    putExtra("experiments", """{ "data": "hello-world" }""")
                },
            ),
        )
        assertNull(
            createCommandLineArgs(
                createIntent().apply {
                    putExtra("experiments", """{ "data": {}}""")
                },
            ),
        )
    }

    private fun createIntent() = Intent("bogus").apply {
        putExtra("nimbus-cli", null as String?)
        putExtra("version", 1)
    }
}
