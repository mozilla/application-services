/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.experiments.nimbus

import android.content.Context
import android.util.Log
import androidx.test.core.app.ApplicationProvider
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import mozilla.telemetry.glean.BuildInfo
import mozilla.telemetry.glean.Glean
import mozilla.telemetry.glean.config.Configuration
import mozilla.telemetry.glean.net.HttpStatus
import mozilla.telemetry.glean.net.PingUploader
import mozilla.telemetry.glean.testing.GleanTestRule
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Ignore
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.mockito.Mockito
import org.mockito.Mockito.`when`
import org.mozilla.experiments.nimbus.GleanMetrics.NimbusEvents
import org.mozilla.experiments.nimbus.GleanMetrics.NimbusHealth
import org.mozilla.experiments.nimbus.internal.EnrollmentChangeEvent
import org.mozilla.experiments.nimbus.internal.EnrollmentChangeEventType
import org.mozilla.experiments.nimbus.internal.GeckoPref
import org.mozilla.experiments.nimbus.internal.GeckoPrefHandler
import org.mozilla.experiments.nimbus.internal.GeckoPrefState
import org.mozilla.experiments.nimbus.internal.JsonObject
import org.mozilla.experiments.nimbus.internal.NimbusException
import org.mozilla.experiments.nimbus.internal.PrefBranch
import org.mozilla.experiments.nimbus.internal.PrefUnenrollReason
import org.mozilla.experiments.nimbus.internal.RecordedContext
import org.mozilla.experiments.nimbus.internal.getCalculatedAttributes
import org.mozilla.experiments.nimbus.internal.validateEventQueries
import org.robolectric.RobolectricTestRunner
import java.io.File
import java.util.Calendar
import java.util.concurrent.Executors

@RunWith(RobolectricTestRunner::class)
@Suppress("LargeClass")
class NimbusTests {
    private val context: Context
        get() = ApplicationProvider.getApplicationContext()

    private val appInfo = NimbusAppInfo(
        appName = "NimbusUnitTest",
        channel = "test",
    )

    private val deviceInfo = NimbusDeviceInfo(
        localeTag = "en-GB",
    )

    private val packageName = context.packageName

    private val nimbusDelegate = NimbusDelegate(
        dbScope = CoroutineScope(Executors.newSingleThreadExecutor().asCoroutineDispatcher()),
        fetchScope = CoroutineScope(Executors.newSingleThreadExecutor().asCoroutineDispatcher()),
        updateScope = null,
        logger = { Log.i("NimbusTest", it) },
        errorReporter = { message, e -> Log.e("NimbusTest", message, e) },
    )

    private var nimbus = createNimbus()

    private fun createNimbus(
        coenrollingFeatureIds: List<String> = listOf(),
        recordedContext: RecordedContext? = null,
        geckoPrefHandler: GeckoPrefHandler? = null,
        block: Nimbus.() -> Unit = {},
    ) = Nimbus(
        context = context,
        appInfo = appInfo,
        coenrollingFeatureIds = coenrollingFeatureIds,
        server = null,
        deviceInfo = deviceInfo,
        observer = null,
        delegate = nimbusDelegate,
        recordedContext = recordedContext,
        geckoPrefHandler = geckoPrefHandler,
    ).also(block)

    @get:Rule
    val gleanRule = GleanTestRule(context)

    @Before
    fun setupGlean() {
        nimbus = createNimbus()
        val buildInfo = BuildInfo(versionCode = "0.0.1", versionName = "0.0.1", buildDate = Calendar.getInstance())

        // Glean needs to be initialized for the experiments API to accept enrollment events, so we
        // init it with a mock client so we don't upload anything.
        val mockClient: PingUploader = mock()
        `when`(mockClient.upload(any())).thenReturn(
            HttpStatus(200),
        )
        Glean.initialize(
            context,
            true,
            Configuration(
                httpClient = mockClient,
            ),
            buildInfo,
        )
    }

    @Test
    fun `recordExperimentTelemetry correctly records the experiment and branch`() {
        // Create a list of experiments to test the telemetry enrollment recording
        val enrolledExperiments = listOf(
            EnrolledExperiment(
                slug = "test-experiment",
                featureIds = listOf(),
                branchSlug = "test-branch",
                userFacingDescription = "A test experiment for testing experiments",
                userFacingName = "Test Experiment",
            ),
        )

        nimbus.recordExperimentTelemetry(experiments = enrolledExperiments)
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
                reason = "test-reason",
                change = EnrollmentChangeEventType.ENROLLMENT,
            ),
            EnrollmentChangeEvent(
                experimentSlug = "test-experiment",
                branchSlug = "test-branch",
                reason = "test-reason",
                change = EnrollmentChangeEventType.UNENROLLMENT,
            ),
            EnrollmentChangeEvent(
                experimentSlug = "test-experiment",
                branchSlug = "test-branch",
                reason = "test-reason",
                change = EnrollmentChangeEventType.DISQUALIFICATION,
            ),
        )

        // Record the experiments in Glean
        nimbus.recordExperimentTelemetryEvents(events)

        // Use the Glean test API to check the recorded metrics

        // Enrollment
        assertNotNull("Event must have a value", NimbusEvents.enrollment.testGetValue())
        val enrollmentEvents = NimbusEvents.enrollment.testGetValue()!!
        assertEquals("Event count must match", enrollmentEvents.count(), 1)
        val enrollmentEventExtras = enrollmentEvents.first().extra!!
        assertEquals(
            "Experiment slug must match",
            "test-experiment",
            enrollmentEventExtras["experiment"],
        )
        assertEquals("Experiment branch must match", "test-branch", enrollmentEventExtras["branch"])

        // Unenrollment
        assertNotNull("Event must have a value", NimbusEvents.unenrollment.testGetValue())
        val unenrollmentEvents = NimbusEvents.unenrollment.testGetValue()!!
        assertEquals("Event count must match", unenrollmentEvents.count(), 1)
        val unenrollmentEventExtras = unenrollmentEvents.first().extra!!
        assertEquals(
            "Experiment slug must match",
            "test-experiment",
            unenrollmentEventExtras["experiment"],
        )
        assertEquals(
            "Experiment branch must match",
            "test-branch",
            unenrollmentEventExtras["branch"],
        )

        // Disqualification
        assertNotNull("Event must have a value", NimbusEvents.disqualification.testGetValue())
        val disqualificationEvents = NimbusEvents.disqualification.testGetValue()!!
        assertEquals("Event count must match", disqualificationEvents.count(), 1)
        val disqualificationEventExtras = disqualificationEvents.first().extra!!
        assertEquals(
            "Experiment slug must match",
            "test-experiment",
            disqualificationEventExtras["experiment"],
        )
        assertEquals(
            "Experiment branch must match",
            "test-branch",
            disqualificationEventExtras["branch"],
        )
    }

    @Test
    fun `getFeatureVariables records activation telemetry`() {
        Glean.applyServerKnobsConfig(
            """{
                "metrics_enabled": {
                    "nimbus_events.activation": true
                }
            }""",
        )
        // Load the experiment in nimbus so and optIn so that it will be active. This is necessary
        // because recordExposure checks for active experiments before recording.
        nimbus.setUpTestExperiments(packageName, appInfo)

        // Assert that there are no events to start with
        assertNull(
            "There must not be any pre-existing events",
            NimbusEvents.activation.testGetValue(),
        )

        nimbus.getVariables("missing_feature", recordExposureEvent = false)
        // Missing feature does not call activation event
        assertNull(
            "There must not be any events for missing_features",
            NimbusEvents.activation.testGetValue(),
        )

        nimbus.getVariables("about_welcome", recordExposureEvent = false)
        // Existing feature does call activation event
        val events = NimbusEvents.activation.testGetValue()
        assertNotNull(
            "There must be an activation event for a feature under experiment",
            events,
        )
        assertEquals("Exactly one activation event", events!!.size, 1)

        val extras = events.first().extra!!
        assertEquals("test-branch", extras["branch"])
        assertEquals("test-experiment", extras["experiment"])
        assertEquals("about_welcome", extras["feature_id"])
    }

    @Test
    fun `getFeatureVariables for coenrolling feature does not record activation telemetry`() {
        // Load the experiment in nimbus so and optIn so that it will be active. This is necessary
        // because recordExposure checks for active experiments before recording.
        val nimbus = createNimbus(coenrollingFeatureIds = listOf("about_welcome"))
        nimbus.setUpTestExperiments(packageName, appInfo)

        // Assert that there are no events to start with
        assertNull(
            "There must not be any pre-existing events",
            NimbusEvents.activation.testGetValue(),
        )

        nimbus.getVariables("missing_feature", recordExposureEvent = false)
        // Missing feature does not call activation event
        assertNull(
            "There must not be any events for missing_features",
            NimbusEvents.activation.testGetValue(),
        )

        nimbus.getVariables("about_welcome", recordExposureEvent = false)
        // Coenrolling feature does not call activation event
        assertNull(
            "There must not be any events for coenrolling feature",
            NimbusEvents.activation.testGetValue(),
        )
    }

    @Test
    fun `recordExposure from feature records telemetry`() {
        // Load the experiment in nimbus so and optIn so that it will be active. This is necessary
        // because recordExposure checks for active experiments before recording.
        nimbus.setUpTestExperiments(packageName, appInfo)

        // Assert that there are no events to start with
        assertNull(
            "There must not be any pre-existing events",
            NimbusEvents.exposure.testGetValue(),
        )

        // Record a valid exposure event in Glean that matches the featureId from the test experiment
        nimbus.recordExposureOnThisThread("about_welcome", null)

        // Use the Glean test API to check that the valid event is present
        assertNotNull("Event must have a value", NimbusEvents.exposure.testGetValue())
        val exposureEvents = NimbusEvents.exposure.testGetValue()!!
        assertEquals("Event count must match", exposureEvents.count(), 1)
        val exposureEventExtras = exposureEvents.first().extra!!
        assertEquals(
            "Experiment slug must match",
            "test-experiment",
            exposureEventExtras["experiment"],
        )
        assertEquals("Experiment branch must match", "test-branch", exposureEventExtras["branch"])

        // Attempt to record an event for a non-existent or feature we are not enrolled in an
        // experiment in to ensure nothing is recorded.
        nimbus.recordExposureOnThisThread("not-a-feature", null)

        // Verify the invalid event was ignored by checking again that the valid event is still the only
        // event, and that it hasn't changed any of its extra properties.
        assertNotNull("Event must have a value", NimbusEvents.exposure.testGetValue())
        val exposureEventsTryTwo = NimbusEvents.exposure.testGetValue()!!
        assertEquals("Event count must match", exposureEventsTryTwo.count(), 1)
        val exposureEventExtrasTryTwo = exposureEventsTryTwo.first().extra!!
        assertEquals(
            "Experiment slug must match",
            "test-experiment",
            exposureEventExtrasTryTwo["experiment"],
        )
        assertEquals(
            "Experiment branch must match",
            "test-branch",
            exposureEventExtrasTryTwo["branch"],
        )
    }

    @Test
    fun `recordExposure from experiment slug records telemetry`() {
        // Load the experiment in nimbus so and optIn so that it will be active. This is necessary
        // because recordExposure checks for active experiments before recording.
        nimbus.setUpTestExperiments(packageName, appInfo)

        // Assert that there are no events to start with
        assertNull(
            "There must not be any pre-existing events",
            NimbusEvents.exposure.testGetValue(),
        )

        // Record a valid exposure event in Glean that matches the featureId from the test experiment
        nimbus.recordExposureOnThisThread("about_welcome", "test-experiment")

        // Use the Glean test API to check that the valid event is present
        assertNotNull("Event must have a value", NimbusEvents.exposure.testGetValue())
        val exposureEvents = NimbusEvents.exposure.testGetValue()!!
        assertEquals("Event count must match", exposureEvents.count(), 1)
        val exposureEventExtras = exposureEvents.first().extra!!
        assertEquals(
            "Experiment slug must match",
            "test-experiment",
            exposureEventExtras["experiment"],
        )
        assertEquals("Experiment branch must match", "test-branch", exposureEventExtras["branch"])

        // Attempt to record an event for a non-existent or feature we are not enrolled in an
        // experiment in to ensure nothing is recorded.
        nimbus.recordExposureOnThisThread("about_welcome", "not-an-experiment")

        // Verify the invalid event was ignored by checking again that the valid event is still the only
        // event, and that it hasn't changed any of its extra properties.
        assertNotNull("Event must have a value", NimbusEvents.exposure.testGetValue())
        val exposureEventsTryTwo = NimbusEvents.exposure.testGetValue()!!
        assertEquals("Event count must match", exposureEventsTryTwo.count(), 1)
        val exposureEventExtrasTryTwo = exposureEventsTryTwo.first().extra!!
        assertEquals(
            "Experiment slug must match",
            "test-experiment",
            exposureEventExtrasTryTwo["experiment"],
        )
        assertEquals(
            "Experiment branch must match",
            "test-branch",
            exposureEventExtrasTryTwo["branch"],
        )
    }

    @Test
    fun `recordMalformedConfiguration records telemetry`() {
        // Load the experiment in nimbus so and optIn so that it will be active. This is necessary
        // because recordExposure checks for active experiments before recording.
        nimbus.setUpTestExperiments(packageName, appInfo)

        // Assert that there are no events to start with
        assertNull(
            "There must not be any pre-existing events",
            NimbusEvents.malformedFeature.testGetValue(),
        )

        // Record a valid exposure event in Glean that matches the featureId from the test experiment
        nimbus.recordMalformedConfigurationOnThisThread("about_welcome", "detail")

        // Use the Glean test API to check that the valid event is present
        assertNotNull("Event must have a value", NimbusEvents.malformedFeature.testGetValue())
        val events = NimbusEvents.malformedFeature.testGetValue()!!
        assertEquals("Event count must match", events.count(), 1)
        val extras = events.first().extra!!
        assertEquals(
            "Experiment slug must match",
            "test-experiment",
            extras["experiment"],
        )
        assertEquals("Experiment branch must match", "test-branch", extras["branch"])
        assertEquals("Feature Id must match", "about_welcome", extras["feature_id"])
        assertEquals("Part Id must match", "detail", extras["part_id"])
    }

    @Test
    fun `opting out generates the correct Glean event`() {
        // Load the experiment in nimbus so and optIn so that it will be active. This is necessary
        // because recordExposure checks for active experiments before recording.
        nimbus.setUpTestExperiments(packageName, appInfo)

        // Assert that there are no events to start with
        assertNull(
            "There must not be any pre-existing events",
            NimbusEvents.disqualification.testGetValue(),
        )

        // Opt out of the specific experiment
        nimbus.optOutOnThisThread("test-experiment")

        // Use the Glean test API to check that the valid event is present
        assertNotNull("Event must have a value", NimbusEvents.disqualification.testGetValue())
        val disqualificationEvents = NimbusEvents.disqualification.testGetValue()!!
        assertEquals("Event count must match", disqualificationEvents.count(), 1)
        val enrollmentEventExtras = disqualificationEvents.first().extra!!
        assertEquals(
            "Experiment slug must match",
            "test-experiment",
            enrollmentEventExtras["experiment"],
        )
        assertEquals("Experiment branch must match", "test-branch", enrollmentEventExtras["branch"])
    }

    @Test
    fun `toggling the global opt out generates the correct Glean event`() {
        // Load the experiment in nimbus so and optIn so that it will be active. This is necessary
        // because recordExposure checks for active experiments before recording.
        nimbus.setUpTestExperiments(packageName, appInfo)

        // Assert that there are no events to start with
        assertNull(
            "There must not be any pre-existing events",
            NimbusEvents.disqualification.testGetValue(),
        )

        // Opt out of all experiments
        nimbus.setGlobalUserParticipationOnThisThread(false)

        // Use the Glean test API to check that the valid event is present
        assertNotNull("Event must have a value", NimbusEvents.disqualification.testGetValue())
        val disqualificationEvents = NimbusEvents.disqualification.testGetValue()!!
        assertEquals("Event count must match", disqualificationEvents.count(), 1)
        val enrollmentEventExtras = disqualificationEvents.first().extra!!
        assertEquals(
            "Experiment slug must match",
            "test-experiment",
            enrollmentEventExtras["experiment"],
        )
        assertEquals("Experiment branch must match", "test-branch", enrollmentEventExtras["branch"])
    }

    private fun Nimbus.setUpTestExperiments(appId: String, appInfo: NimbusAppInfo) {
        this.setExperimentsLocallyOnThisThread(
            testExperimentsJsonString(appInfo, appId),
        )
        this.applyPendingExperimentsOnThisThread()
    }

    private fun testExperimentsJsonString(
        appInfo: NimbusAppInfo,
        appId: String,
    ) = """
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
    """.trimIndent()

    @Test
    fun `buildExperimentContext returns a valid context`() {
        val expContext = nimbus.buildExperimentContext(context, appInfo, deviceInfo)
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
            appInfo = developmentAppInfo,
            coenrollingFeatureIds = listOf(),
            server = null,
            deviceInfo = deviceInfo,
            delegate = nimbusDelegate,
        )

        nimbus.setUpTestExperiments("$packageName.nightly", targetedAppInfo)

        val available: List<AvailableExperiment> = nimbus.getAvailableExperiments()
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
            coenrollingFeatureIds = listOf(),
            server = null,
            deviceInfo = deviceInfo,
            delegate = nimbusDelegate,
        )

        nimbus.setUpTestExperiments(packageName, targetedAppInfo)

        val available = nimbus.getAvailableExperiments()
        assertTrue(available.isEmpty())
    }

    @Test
    fun `applyLocalExperiments calls setLocalExperiments and applyPendingExperiments`() {
        var completed = false
        suspend fun getString(): String {
            completed = true
            return testExperimentsJsonString(appInfo, packageName)
        }

        val job = nimbus.applyLocalExperiments(::getString)
        runBlocking {
            job.join()
        }

        assertTrue(completed)
        assertEquals(1, nimbus.getAvailableExperiments().size)
    }

    @Test
    @Ignore
    fun `in memory cache not ready logs an event`() {
        // we haven't initialized nimbus at all, it should not log any error, but it should log an
        // event
        assertNull(nimbus.getFeatureConfigVariablesJson("dummy-experiment"))
        assertNotNull(NimbusHealth.cacheNotReadyForFeature.testGetValue())
    }

    @Test
    fun `applyLocalExperiments is cancellable`() {
        var completed = false
        suspend fun getString(): String =
            throw CancellationException()

        val job = nimbus.applyLocalExperiments(::getString)
        runBlocking {
            job.cancelAndJoin()
        }

        assertFalse(completed)
        assertEquals(0, nimbus.getAvailableExperiments().size)
        // this should not throw a DatabaseNotReadyException.
        assertNull(nimbus.getFeatureConfigVariablesJson("dummy-experiment"))
        assertNull(NimbusHealth.cacheNotReadyForFeature.testGetValue())
    }

    @Test
    fun `applyLocalExperiments is cancellable with timeout`() {
        var completed = false
        suspend fun getString(): String {
            delay(1000)
            completed = true
            return testExperimentsJsonString(appInfo, packageName)
        }

        val job = nimbus.applyLocalExperiments(::getString)
        runBlocking {
            job.joinOrTimeout(250L)
        }

        assertFalse(completed)
        assertEquals(0, nimbus.getAvailableExperiments().size)
        // this should not throw a DatabaseNotReadyException.
        assertNull(nimbus.getFeatureConfigVariablesJson("dummy-experiment"))
        assertNull(NimbusHealth.cacheNotReadyForFeature.testGetValue())
    }

    @Test
    fun `test observers are not cancelled`() {
        var observed = false
        val observer = object : NimbusInterface.Observer {
            override fun onUpdatesApplied(updated: List<org.mozilla.experiments.nimbus.internal.EnrolledExperiment>) {
                runBlocking {
                    delay(250)
                    observed = true
                }
            }
        }
        val nimbus = Nimbus(
            context = context,
            appInfo = appInfo,
            coenrollingFeatureIds = listOf(),
            server = null,
            deviceInfo = deviceInfo,
            observer = observer,
            delegate = nimbusDelegate,
        )

        suspend fun getString() = testExperimentsJsonString(appInfo, packageName)

        val job = nimbus.applyLocalExperiments(::getString)
        runBlocking {
            job.joinOrTimeout(100)
        }

        assertTrue(observed)
    }

    @Test
    fun `test observers are not cancelled even if loading is cancelled`() {
        var observed = false
        val observer = object : NimbusInterface.Observer {
            override fun onUpdatesApplied(updated: List<org.mozilla.experiments.nimbus.internal.EnrolledExperiment>) {
                runBlocking {
                    delay(250)
                    observed = true
                }
            }
        }
        val nimbus = Nimbus(
            context = context,
            appInfo = appInfo,
            coenrollingFeatureIds = listOf(),
            server = null,
            deviceInfo = deviceInfo,
            observer = observer,
            delegate = nimbusDelegate,
        )

        suspend fun getString(): String = throw CancellationException()

        val job = nimbus.applyLocalExperiments(::getString)
        runBlocking {
            job.joinOrTimeout(100)
        }

        assertTrue(observed)
    }

    @Test
    @Ignore
    fun `Nimbus records EnrollmentStatus metrics`() {
        suspend fun getString(): String {
            return testExperimentsJsonString(appInfo, packageName)
        }

        val job = nimbus.applyLocalExperiments(::getString)
        runBlocking {
            job.join()
        }

        assertEquals(1, nimbus.getAvailableExperiments().size)
        assertNotNull("Event must have a value", NimbusEvents.enrollmentStatus.testGetValue())
        val enrollmentStatusEvents = NimbusEvents.enrollmentStatus.testGetValue()!!
        assertEquals("Event count must match", enrollmentStatusEvents.count(), 1)

        val enrolledExtra = enrollmentStatusEvents[0].extra!!
        assertEquals("branch must match", "test-branch", enrolledExtra["branch"])
        assertEquals("slug must match", "test-experiment", enrolledExtra["slug"])
        assertEquals("status must match", "Enrolled", enrolledExtra["status"])
        assertEquals("reason must match", "Qualified", enrolledExtra["reason"])
        assertEquals("errorString must match", null, enrolledExtra["error_string"])
        assertEquals("conflictSlug must match", null, enrolledExtra["conflict_slug"])
    }

    @Test
    fun `recordIsReady records the correct number of is_ready events`() {
        nimbus.recordIsReady(1)
        var isReadyEvents = NimbusEvents.isReady.testGetValue()!!
        assertEquals("Event count must match", isReadyEvents.count(), 1)

        nimbus.recordIsReady(2)
        isReadyEvents = NimbusEvents.isReady.testGetValue()!!
        assertEquals("Event count must match", isReadyEvents.count(), 3)
    }

    class TestRecordedContext(
        private val eventQueries: Map<String, String>? = null,
        private var eventQueryValues: Map<String, Double>? = null,
    ) : RecordedContext {
        var recorded = mutableListOf<JSONObject>()

        override fun getEventQueries(): Map<String, String> {
            return eventQueries?.toMap() ?: mapOf()
        }

        override fun setEventQueryValues(eventQueryValues: Map<String, Double>) {
            this.eventQueryValues = eventQueryValues
        }

        override fun record() {
            recorded.add(this.toJson())
        }

        override fun toJson(): JsonObject {
            val contextToRecord = JSONObject()
            contextToRecord.put("enabled", true)
            contextToRecord.put("events", JSONObject(eventQueryValues ?: mapOf<String, Double>()))
            return contextToRecord
        }
    }

    @Test
    fun `Nimbus records context if it's passed in`() {
        val context = TestRecordedContext()
        val nimbus = createNimbus(recordedContext = context)

        suspend fun getString(): String {
            return testExperimentsJsonString(appInfo, packageName)
        }

        val job = nimbus.applyLocalExperiments(::getString)
        runBlocking {
            job.join()
        }

        assertEquals(context.recorded.size, 1)
    }

    @Test
    fun `Nimbus recorded context event queries are run and the value is written back into the object`() {
        val context = TestRecordedContext(
            mapOf(
                "TEST_QUERY" to "'event'|eventSum('Days', 1, 0)",
            ),
        )
        val nimbus = createNimbus(recordedContext = context) {
            recordEvent("event")
        }

        suspend fun getString(): String {
            return testExperimentsJsonString(appInfo, packageName)
        }

        val job = nimbus.applyLocalExperiments(::getString)
        runBlocking {
            job.join()
        }

        assertEquals(context.recorded.size, 1)
        assertEquals(context.recorded[0].getJSONObject("events").getDouble("TEST_QUERY"), 1.0, 0.0)
    }

    @Test
    fun `Nimbus recorded context event queries are validated`() {
        val context = TestRecordedContext(
            mapOf(
                "FAILING_QUERY" to "'event'|eventSumThisWillFail('Days', 1, 0)",
            ),
        )

        assertThrows("Expected an error to be thrown", NimbusException::class.java, { validateEventQueries(context) })
    }

    @Test
    fun `Nimbus can obtain calculated attributes`() {
        val db = File(context.applicationInfo.dataDir, NIMBUS_DATA_DIR)
        val deviceInfo = NimbusDeviceInfo.default()
        val date = Calendar.getInstance()

        val calculatedAttributes = getCalculatedAttributes(date.timeInMillis - 86400000 * 5, db.path, deviceInfo.localeTag)

        assertEquals(5, calculatedAttributes.daysSinceInstall)
        assertEquals(0, calculatedAttributes.daysSinceUpdate)
        assertEquals("en", calculatedAttributes.language)
        assertEquals("US", calculatedAttributes.region)
    }

    class TestGeckoPrefHandler(
        var internalMap: Map<String, Map<String, GeckoPrefState>> = mapOf(
            "about_welcome" to mapOf(
                "number" to GeckoPrefState(
                    geckoPref = GeckoPref("pref.number", PrefBranch.DEFAULT),
                    geckoValue = "1",
                    enrollmentValue = null,
                    isUserSet = false,
                ),
            ),
        ),
        var setValues: List<GeckoPrefState>? = null,
    ) : GeckoPrefHandler {
        override fun getPrefsWithState(): Map<String, Map<String, GeckoPrefState>> {
            return internalMap
        }

        override fun setGeckoPrefsState(newPrefsState: List<GeckoPrefState>) {
            setValues = newPrefsState
        }
    }

    @Test
    fun `GeckoPrefHandler functions`() {
        val handler = TestGeckoPrefHandler()

        val nimbus = createNimbus(geckoPrefHandler = handler)

        suspend fun getString(): String {
            return testExperimentsJsonString(appInfo, packageName)
        }

        val job = nimbus.applyLocalExperiments(::getString)
        runBlocking {
            job.join()
        }

        assertEquals(1, handler.setValues?.size)
        assertEquals("42", handler.setValues?.get(0)?.enrollmentValue?.prefValue)
    }

    @Test
    fun `unenroll for gecko pref functions`() {
        val handler = TestGeckoPrefHandler()

        val nimbus = createNimbus(geckoPrefHandler = handler)

        suspend fun getString(): String {
            return testExperimentsJsonString(appInfo, packageName)
        }

        val job = nimbus.applyLocalExperiments(::getString)
        runBlocking {
            job.join()
        }

        assertEquals(1, handler.setValues?.size)
        assertEquals("42", handler.setValues?.get(0)?.enrollmentValue?.prefValue)

        val events = nimbus.unenrollForGeckoPref(handler.internalMap["about_welcome"]?.get("number")!!, PrefUnenrollReason.FAILED_TO_SET)

        assertEquals(1, events.size)
        assertEquals(EnrollmentChangeEventType.DISQUALIFICATION, events[0].change)
        assertEquals(0, handler.setValues?.size)
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
