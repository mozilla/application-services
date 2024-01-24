/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.experiments.nimbus

import android.content.Context
import android.util.Log
import androidx.test.core.app.ApplicationProvider
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.asCoroutineDispatcher
import org.json.JSONObject
import org.junit.Assert
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.mozilla.experiments.nimbus.internal.NimbusException
import org.robolectric.RobolectricTestRunner
import java.util.UUID
import java.util.concurrent.Executors

@RunWith(RobolectricTestRunner::class)
class MessagingHelperTests {
    private val context: Context
        get() = ApplicationProvider.getApplicationContext()

    private val deviceInfo = NimbusDeviceInfo(
        localeTag = "en-GB",
    )

    private val nimbusDelegate = NimbusDelegate(
        dbScope = CoroutineScope(Executors.newSingleThreadExecutor().asCoroutineDispatcher()),
        fetchScope = CoroutineScope(Executors.newSingleThreadExecutor().asCoroutineDispatcher()),
        logger = { Log.i("NimbusTest", it) },
        errorReporter = { message, e -> Log.e("NimbusTest", message, e) },
    )

    @Test
    fun `jexl can be run against the targeting attributes`() {
        val developmentAppInfo = NimbusAppInfo(appName = "ThatApp", channel = "production")

        val nimbus = Nimbus(
            context = context,
            appInfo = developmentAppInfo,
            coenrollingFeatureIds = listOf(),
            server = null,
            deviceInfo = deviceInfo,
            delegate = nimbusDelegate,
        )
        nimbus.initializeOnThisThread()

        val messageHelper = nimbus.createMessageHelper()
        // Evaluate two different expressions that give true and false answers
        // to prove we're actually parsing JEXL, rather than always returning true.
        assertTrue(messageHelper.evalJexl("app_name == 'ThatApp'"))
        assertFalse(messageHelper.evalJexl("app_name == 'ppAtahT'"))

        // The JEXL evaluator should error for unknown identifiers
        assertThrows("invalid jexl", NimbusException::class.java) {
            messageHelper.evalJexl("appName == 'ThatApp'")
        }
    }

    @Test
    fun `jexl can be run against the json context`() {
        val developmentAppInfo = NimbusAppInfo(appName = "ThatApp", channel = "production")

        val nimbus = Nimbus(
            context = context,
            appInfo = developmentAppInfo,
            coenrollingFeatureIds = listOf(),
            server = null,
            deviceInfo = deviceInfo,
            delegate = nimbusDelegate,
        )
        nimbus.initializeOnThisThread()

        // Evaluate two different expressions that give true and false answers
        // to prove we're actually parsing JEXL, rather than always returning true.
        val context = JSONObject(
            """{
                    "test_value_from_json": 42
                }
            """.trimIndent(),
        )

        assertThrows("no context, so no variable", NimbusException::class.java) {
            val messageHelper = nimbus.createMessageHelper()
            messageHelper.evalJexl("test_value_from_json == 42")
        }

        val messageHelper = nimbus.createMessageHelper(context)
        assertTrue(
            messageHelper.evalJexl(
                "test_value_from_json == 42",
            ),
        )
    }

    @Test
    fun `test string helper shows a uuid`() {
        val developmentAppInfo = NimbusAppInfo(appName = "ThatApp", channel = "production")

        val nimbus = Nimbus(
            context = context,
            appInfo = developmentAppInfo,
            coenrollingFeatureIds = listOf(),
            server = null,
            deviceInfo = deviceInfo,
            delegate = nimbusDelegate,
        )
        nimbus.initializeOnThisThread()

        // We're going to substitute variables from the app context and the json context
        val context = JSONObject(
            """{
                    "test_string": "foobar"
                }
            """.trimIndent(),
        )

        val helper = nimbus.createMessageHelper(context)
        val t1 = "{test_string} for {app_name}"

        assertNull(helper.getUuid(t1))
        assertEquals(helper.stringFormat(t1), "foobar for ThatApp")

        // We're also going to show we can use UUIDs
        val t2 = "{uuid}"
        val uuid = helper.getUuid(t2)
        assertNotNull(uuid)
        try {
            UUID.fromString(uuid!!)
            UUID.fromString(helper.stringFormat(t2, uuid))
        } catch (e: IllegalArgumentException) {
            Assert.fail("Not a valid UUID given by {uuid}")
        }
    }
}
