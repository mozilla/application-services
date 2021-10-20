/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.experiments.nimbus

import android.content.Context
import android.util.Log
import androidx.test.core.app.ApplicationProvider
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.asCoroutineDispatcher
import mozilla.components.concept.fetch.Client
import mozilla.components.concept.fetch.Response
import mozilla.components.service.glean.BuildInfo
import mozilla.components.service.glean.Glean
import mozilla.components.service.glean.config.Configuration
import mozilla.components.service.glean.net.ConceptFetchHttpUploader
import mozilla.components.service.glean.testing.GleanTestRule
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.mockito.Mockito
import org.mockito.Mockito.`when`
import org.mozilla.experiments.nimbus.GleanMetrics.NimbusEvents
import org.mozilla.experiments.nimbus.internal.EnrollmentChangeEvent
import org.mozilla.experiments.nimbus.internal.EnrollmentChangeEventType
import org.mozilla.experiments.nimbus.internal.NimbusClient
import org.mozilla.experiments.nimbus.internal.NimbusClientDecorator
import org.robolectric.RobolectricTestRunner
import java.util.concurrent.Executors

@RunWith(RobolectricTestRunner::class)
class NimbusTest {
    private val context: Context
        get() = ApplicationProvider.getApplicationContext()

    private val appInfo = NimbusAppInfo(
        appName = "NimbusUnitTest",
        channel = "test"
    )

    private val deviceInfo = NimbusDeviceInfo(
        localeTag = "en-GB"
    )

    private val packageName = context.packageName

    private val nimbusDelegate = NimbusDelegate(
        dbScope = CoroutineScope(Executors.newSingleThreadExecutor().asCoroutineDispatcher()),
        fetchScope = CoroutineScope(Executors.newSingleThreadExecutor().asCoroutineDispatcher()),
        logger = { Log.i("NimbusTest", it) },
        errorReporter = { message, e -> Log.e("NimbusTest", message, e) }
    )

    private val nimbusDecorator = object : NimbusClientDecorator<NimbusClient> {
        override fun <ReturnType> onDbThread(obj: NimbusClient, thunk: () -> ReturnType) {
            withCatchAll(obj, thunk)
        }

        override fun <ReturnType> onNetworkThread(obj: NimbusClient, thunk: () -> ReturnType) {
            withCatchAll(obj, thunk)
        }

        override fun <ReturnType> withCatchAll(
            obj: NimbusClient,
            thunk: () -> ReturnType
        ): ReturnType? = thunk()

        override fun <ReturnType> onEnrollmentChanges(obj: NimbusClient, thunk: () -> ReturnType): Unit = onDbThread(obj, thunk)
    }

    private val nimbus = Nimbus(
        context = context,
        decorator = nimbusDecorator,
        appInfo = appInfo,
        server = null,
        deviceInfo = deviceInfo
    )

    @get:Rule
    val gleanRule = GleanTestRule(context)

    @Before
    fun setupGlean() {
        val buildInfo = BuildInfo(versionCode = "0.0.1", versionName = "0.0.1")

        // Glean needs to be initialized for the experiments API to accept enrollment events, so we
        // init it with a mock client so we don't upload anything.
        val mockClient: Client = mock()
        `when`(mockClient.fetch(any())).thenReturn(
            Response("URL", 200, mock(), mock()))
        Glean.initialize(
            context,
            true,
            Configuration(
                httpClient = ConceptFetchHttpUploader(lazy { mockClient })
            ),
            buildInfo
        )
    }

    @Test
    fun `recordExperimentTelemetry correctly records the experiment and branch`() {
        // Create a list of experiments to test the telemetry enrollment recording
        val enrolledExperiments = listOf(
            EnrolledExperiment(
                enrollmentId = "enrollment-id",
                slug = "test-experiment",
                featureIds = listOf(),
                branchSlug = "test-branch",
                userFacingDescription = "A test experiment for testing experiments",
                userFacingName = "Test Experiment"
            )
        )

        val decorator = NimbusDecorator(nimbusDelegate)
        decorator.recordExperimentTelemetry(experiments = enrolledExperiments)
        assertTrue(Glean.testIsExperimentActive("test-experiment"))
        val experimentData = Glean.testGetExperimentData("test-experiment")
        assertEquals("test-branch", experimentData.branch)
    }

    @Test
    fun `recordExperimentTelemetryEvents records telemetry`() {
        // Create a bespoke list of events to record, one of each type, all with the same parameters
        val events = listOf(
            EnrollmentChangeEvent(
                experimentSlug = "test-experiment",
                branchSlug = "test-branch",
                enrollmentId = "test-enrollment-id",
                reason = "test-reason",
                change = EnrollmentChangeEventType.ENROLLMENT
            ),
            EnrollmentChangeEvent(
                experimentSlug = "test-experiment",
                branchSlug = "test-branch",
                enrollmentId = "test-enrollment-id",
                reason = "test-reason",
                change = EnrollmentChangeEventType.UNENROLLMENT
            ),
            EnrollmentChangeEvent(
                experimentSlug = "test-experiment",
                branchSlug = "test-branch",
                enrollmentId = "test-enrollment-id",
                reason = "test-reason",
                change = EnrollmentChangeEventType.DISQUALIFICATION
            )
        )

        // Record the experiments in Glean
        val decorator = NimbusDecorator(nimbusDelegate)
        decorator.recordExperimentTelemetryEvents(events)

        // Use the Glean test API to check the recorded metrics

        // Enrollment
        assertTrue("Event must have a value", NimbusEvents.enrollment.testHasValue())
        val enrollmentEvents = NimbusEvents.enrollment.testGetValue()
        assertEquals("Event count must match", enrollmentEvents.count(), 1)
        val enrollmentEventExtras = enrollmentEvents.first().extra!!
        assertEquals("Experiment slug must match", "test-experiment", enrollmentEventExtras["experiment"])
        assertEquals("Experiment branch must match", "test-branch", enrollmentEventExtras["branch"])
        assertEquals("Experiment enrollment-id must match", "test-enrollment-id", enrollmentEventExtras["enrollment_id"])

        // Unenrollment
        assertTrue("Event must have a value", NimbusEvents.unenrollment.testHasValue())
        val unenrollmentEvents = NimbusEvents.unenrollment.testGetValue()
        assertEquals("Event count must match", unenrollmentEvents.count(), 1)
        val unenrollmentEventExtras = unenrollmentEvents.first().extra!!
        assertEquals("Experiment slug must match", "test-experiment", unenrollmentEventExtras["experiment"])
        assertEquals("Experiment branch must match", "test-branch", unenrollmentEventExtras["branch"])
        assertEquals("Experiment enrollment-id must match", "test-enrollment-id", unenrollmentEventExtras["enrollment_id"])

        // Disqualification
        assertTrue("Event must have a value", NimbusEvents.disqualification.testHasValue())
        val disqualificationEvents = NimbusEvents.disqualification.testGetValue()
        assertEquals("Event count must match", disqualificationEvents.count(), 1)
        val disqualificationEventExtras = disqualificationEvents.first().extra!!
        assertEquals("Experiment slug must match", "test-experiment", disqualificationEventExtras["experiment"])
        assertEquals("Experiment branch must match", "test-branch", disqualificationEventExtras["branch"])
        assertEquals("Experiment enrollment-id must match", "test-enrollment-id", disqualificationEventExtras["enrollment_id"])
    }

    @Test
    fun `recordExposure records telemetry`() {
        // Load the experiment in nimbus so and optIn so that it will be active. This is necessary
        // because recordExposure checks for active experiments before recording.
        nimbus.setUpTestExperiments(packageName, appInfo)

        // Assert that there are no events to start with
        assertFalse("There must not be any pre-existing events", NimbusEvents.exposure.testHasValue())

        // Record a valid exposure event in Glean that matches the featureId from the test experiment
        nimbus.recordExposureEvent("about_welcome")

        // Use the Glean test API to check that the valid event is present
        assertTrue("Event must have a value", NimbusEvents.exposure.testHasValue())
        val enrollmentEvents = NimbusEvents.exposure.testGetValue()
        assertEquals("Event count must match", enrollmentEvents.count(), 1)
        val enrollmentEventExtras = enrollmentEvents.first().extra!!
        assertEquals("Experiment slug must match", "test-experiment", enrollmentEventExtras["experiment"])
        assertEquals("Experiment branch must match", "test-branch", enrollmentEventExtras["branch"])
        assertNotNull("Experiment enrollment-id must not be null", enrollmentEventExtras["enrollment_id"])

        // Attempt to record an event for a non-existent or feature we are not enrolled in an
        // experiment in to ensure nothing is recorded.
        nimbus.recordExposureEvent("not-a-feature")

        // Verify the invalid event was ignored by checking again that the valid event is still the only
        // event, and that it hasn't changed any of its extra properties.
        assertTrue("Event must have a value", NimbusEvents.exposure.testHasValue())
        val enrollmentEventsTryTwo = NimbusEvents.exposure.testGetValue()
        assertEquals("Event count must match", enrollmentEventsTryTwo.count(), 1)
        val enrollmentEventExtrasTryTwo = enrollmentEventsTryTwo.first().extra!!
        assertEquals("Experiment slug must match", "test-experiment", enrollmentEventExtrasTryTwo["experiment"])
        assertEquals("Experiment branch must match", "test-branch", enrollmentEventExtrasTryTwo["branch"])
        assertNotNull("Experiment enrollment-id must not be null", enrollmentEventExtrasTryTwo["enrollment_id"])
    }

    private fun Nimbus.setUpTestExperiments(appId: String, appInfo: NimbusAppInfo) {
        this.setExperimentsLocally("""
                {"data": [{
                  "schemaVersion": "1.0.0",
                  "slug": "test-experiment",
                  "endDate": null,
                  "featureIds": ["about_welcome"],
                  "branches": [
                    {
                      "slug": "test-branch",
                      "ratio": 1,
                      "feature": {
                          "featureId": "about_welcome",
                          "enabled": false,
                          "value": {
                            "text": "OK then",
                            "number": 42
                          }
                      }
                    }
                  ],
                  "probeSets": [],
                  "startDate": null,
                  "appName": "${appInfo.appName}",
                  "appId": "$appId",
                  "channel": "${appInfo.channel}",
                  "bucketConfig": {
                    "count": 10000,
                    "start": 0,
                    "total": 10000,
                    "namespace": "test-experiment",
                    "randomizationUnit": "nimbus_id"
                  },
                  "userFacingName": "Diagnostic test experiment",
                  "referenceBranch": "test-branch",
                  "isEnrollmentPaused": false,
                  "proposedEnrollment": 7,
                  "userFacingDescription": "This is a test experiment for diagnostic purposes.",
                  "id": "test-experiment",
                  "last_modified": 1602197324372
                }]}
            """.trimIndent())

        this.applyPendingExperiments()
    }

    @Test
    fun `buildExperimentContext returns a valid context`() {
        val expContext = Nimbus.buildExperimentContext(context, appInfo, deviceInfo)
        assertEquals(packageName, expContext.appId)
        assertEquals(appInfo.appName, expContext.appName)
        assertEquals(appInfo.channel, expContext.channel)
        // If we could control more of the context here we might be able to better test it
    }

    @Test
    fun `Smoke test receiving JSON features`() {
        nimbus.setUpTestExperiments(packageName, appInfo)
        // The test experiment has exactly one branch with 100% enrollment
        // We should be able to get feature variables for the feature in this
        // experiment.
        val json = nimbus.getFeatureConfigVariablesJson("about_welcome")
        assertNotNull(json)
        assertEquals(42, json!!["number"])
        assertEquals("OK then", json["text"])

        val json2 = nimbus.getFeatureConfigVariablesJson("non-existent-feature")
        assertNull(json2)
    }

    @Test
    fun `getAvailableExperiments returns experiments for this appName even if the channel and appId don't match`() {
        val appName = "TestApp"
        val targetedAppInfo = NimbusAppInfo(appName = appName, channel = "production")
        val developmentAppInfo = NimbusAppInfo(appName = appName, channel = "developer")

        val nimbus = Nimbus(
            context = context,
            decorator = nimbusDecorator,
            appInfo = developmentAppInfo,
            server = null,
            deviceInfo = deviceInfo
        )

        nimbus.setUpTestExperiments("$packageName.nightly", targetedAppInfo)

        val available: List<AvailableExperiment> = nimbus.getAvailableExperiments() ?: throw AssertionError("Rust error")
        assertEquals(1, available.size)
        assertEquals("test-experiment", available.first().slug)
    }

    @Test
    fun `getAvailableExperiments does not return experiments that don't match the appName`() {
        val targetedAppInfo = NimbusAppInfo(appName = "ThisApp", channel = "production")
        val developmentAppInfo = NimbusAppInfo(appName = "ThatApp", channel = "production")

        val nimbus = Nimbus(
            context = context,
            appInfo = developmentAppInfo,
            server = null,
            deviceInfo = deviceInfo,
            delegate = nimbusDelegate
        )

        nimbus.setUpTestExperiments(packageName, targetedAppInfo)

        val available = nimbus.getAvailableExperiments() ?: throw AssertionError("Rust error")
        assertTrue(available.isEmpty())
    }
}

// Mocking utilities, from mozilla.components.support.test
fun <T> any(): T {
    Mockito.any<T>()
    return uninitialized()
}

@Suppress("UNCHECKED_CAST")
fun <T> uninitialized(): T = null as T

inline fun <reified T : Any> mock(): T = Mockito.mock(T::class.java)
