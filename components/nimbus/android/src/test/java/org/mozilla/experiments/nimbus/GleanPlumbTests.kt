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
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.mozilla.experiments.nimbus.internal.AvailableRandomizationUnits
import org.mozilla.experiments.nimbus.internal.NimbusClient
import org.mozilla.experiments.nimbus.internal.NimbusException
import org.robolectric.RobolectricTestRunner
import java.util.concurrent.Executors

@RunWith(RobolectricTestRunner::class)
class GleanPlumbTests {
    private val context: Context
        get() = ApplicationProvider.getApplicationContext()

    private val deviceInfo = NimbusDeviceInfo(
        localeTag = "en-GB"
    )

    private val nimbusDelegate = NimbusDelegate(
        dbScope = CoroutineScope(Executors.newSingleThreadExecutor().asCoroutineDispatcher()),
        fetchScope = CoroutineScope(Executors.newSingleThreadExecutor().asCoroutineDispatcher()),
        logger = { Log.i("NimbusTest", it) },
        errorReporter = { message, e -> Log.e("NimbusTest", message, e) }
    )

    @Test
    fun `jexl can be run against the targeting attributes`() {
        val developmentAppInfo = NimbusAppInfo(appName = "ThatApp", channel = "production")

        val nimbus = Nimbus(
            context = context,
            appInfo = developmentAppInfo,
            server = null,
            deviceInfo = deviceInfo,
            delegate = nimbusDelegate
        )
        nimbus.initializeOnThisThread()

        val messageHelper = nimbus.createMessageHelper()
        // Evaluate two different expressions that give true and false answers
        // to prove we're actually parsing JEXL, rather than always returning true.
        assertTrue(messageHelper.evalJexl("app_name == 'ThatApp'"))
        assertFalse(messageHelper.evalJexl("app_name == 'ppAtahT'"))

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
            server = null,
            deviceInfo = deviceInfo,
            delegate = nimbusDelegate
        )
        nimbus.initializeOnThisThread()

        val messageHelper = nimbus.createMessageHelper()
        // Evaluate two different expressions that give true and false answers
        // to prove we're actually parsing JEXL, rather than always returning true.
        val context = JSONObject(
            """{
                    "test_value_from_json": 42
                }""".trimIndent()
        )

        assertThrows("invalid jexl", NimbusException::class.java) {
            messageHelper.evalJexl("test_value_from_json == 42")
        }

        assertTrue(
            messageHelper.evalJexl(
                "test_value_from_json == 42",
                context
            )
        )
    }

    fun `invalid json throws an exception`() {
        // This is only a problem if we allow access to strings being sent directly into Rust.
        val developmentAppInfo = NimbusAppInfo(appName = "ThatApp", channel = "production")

        val nimbus = Nimbus(
            context = context,
            appInfo = developmentAppInfo,
            server = null,
            deviceInfo = deviceInfo,
            delegate = nimbusDelegate
        )
        nimbus.initializeOnThisThread()

        val nimbusClient = NimbusClient(
            nimbus.buildExperimentContext(context, developmentAppInfo, deviceInfo),
            context.dataDir.absolutePath + "/test-path",
            null,
            AvailableRandomizationUnits(null, 0)
        )

        val helper = nimbusClient.createTargetingHelper()
        assertTrue(helper.evalJexl("true", null))
        assertTrue(helper.evalJexl("true", "{}"))

        assertThrows("invalid json", NimbusException::class.java) {
            helper.evalJexl("true", "{")
        }

        assertThrows("not an object", NimbusException::class.java) {
            helper.evalJexl("true", "[]")
        }

        assertThrows("not an object", NimbusException::class.java) {
            helper.evalJexl("true", "1")
        }

        assertThrows("not an object", NimbusException::class.java) {
            helper.evalJexl("true", "true")
        }
    }
}
