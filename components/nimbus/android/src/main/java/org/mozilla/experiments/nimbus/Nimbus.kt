/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

@file:Suppress("TooManyFunctions")

package org.mozilla.experiments.nimbus

import android.content.Context
import android.content.SharedPreferences
import android.content.pm.PackageInfo
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import androidx.annotation.AnyThread
import androidx.annotation.RawRes
import androidx.annotation.VisibleForTesting
import androidx.annotation.WorkerThread
import androidx.core.content.pm.PackageInfoCompat
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import mozilla.appservices.remotesettings.RemoteSettingsConfig
import mozilla.telemetry.glean.Glean
import org.json.JSONObject
import org.mozilla.experiments.nimbus.GleanMetrics.NimbusEvents
import org.mozilla.experiments.nimbus.GleanMetrics.NimbusHealth
import org.mozilla.experiments.nimbus.internal.AppContext
import org.mozilla.experiments.nimbus.internal.AvailableExperiment
import org.mozilla.experiments.nimbus.internal.AvailableRandomizationUnits
import org.mozilla.experiments.nimbus.internal.EnrolledExperiment
import org.mozilla.experiments.nimbus.internal.EnrollmentChangeEvent
import org.mozilla.experiments.nimbus.internal.EnrollmentChangeEventType
import org.mozilla.experiments.nimbus.internal.NimbusClient
import org.mozilla.experiments.nimbus.internal.NimbusClientInterface
import org.mozilla.experiments.nimbus.internal.NimbusException
import java.io.File
import java.io.IOException

private const val EXPERIMENT_COLLECTION_NAME = "nimbus-mobile-experiments"
private const val NIMBUS_DATA_DIR: String = "nimbus_data"

/**
 * This class allows client apps to configure Nimbus to point to your own server.
 * Client app developers should set up their own Nimbus infrastructure, to avoid different
 * organizations running conflicting experiments or hitting servers with extra network traffic.
 */
data class NimbusServerSettings(
    val url: Uri,
    val collection: String = EXPERIMENT_COLLECTION_NAME,
)

/**
 * A implementation of the [NimbusInterface] interface backed by the Nimbus SDK.
 */
@Suppress("LargeClass", "LongParameterList")
open class Nimbus(
    override val context: Context,
    override val prefs: SharedPreferences? = null,
    appInfo: NimbusAppInfo,
    coenrollingFeatureIds: List<String>,
    server: NimbusServerSettings?,
    deviceInfo: NimbusDeviceInfo,
    private val observer: NimbusInterface.Observer? = null,
    delegate: NimbusDelegate,
) : NimbusInterface {
    // An I/O scope is used for reading or writing from the Nimbus's RKV database.
    private val dbScope: CoroutineScope = delegate.dbScope

    // An I/O scope is used for getting experiments from the network.
    private val fetchScope: CoroutineScope = delegate.fetchScope

    private val updateScope: CoroutineScope? = delegate.updateScope

    private val errorReporter = delegate.errorReporter

    private val logger = delegate.logger

    private val nimbusClient: NimbusClientInterface

    override var globalUserParticipation: Boolean
        get() = nimbusClient.getGlobalUserParticipation()
        set(active) {
            dbScope.launch {
                setGlobalUserParticipationOnThisThread(active)
            }
        }

    init {
        NullVariables.instance.setContext(context)

        // Set the name of the native library so that we use
        // the appservices megazord for compiled code.
        System.setProperty(
            "uniffi.component.nimbus.libraryOverride",
            System.getProperty("mozilla.appservices.megazord.library", "megazord"),
        )
        // Build a File object to represent the data directory for Nimbus data
        val dataDir = File(context.applicationInfo.dataDir, NIMBUS_DATA_DIR)

        // Build Nimbus AppContext object to pass into initialize
        val experimentContext = buildExperimentContext(context, appInfo, deviceInfo)

        // Initialize Nimbus
        val remoteSettingsConfig = server?.let {
            RemoteSettingsConfig(
                serverUrl = it.url.toString(),
                collectionName = it.collection,
            )
        }

        nimbusClient = NimbusClient(
            experimentContext,
            coenrollingFeatureIds,
            dataDir.path,
            remoteSettingsConfig,
            // The "dummy" field here is required for obscure reasons when generating code on desktop,
            // so we just automatically set it to a dummy value.
            AvailableRandomizationUnits(clientId = null, userId = null, nimbusId = null, dummy = 0),
        )
    }

    // This is currently not available from the main thread.
    // see https://jira.mozilla.com/browse/SDK-191
    @WorkerThread
    override fun getActiveExperiments(): List<EnrolledExperiment> = withCatchAll("getActiveExperiments") {
        nimbusClient.getActiveExperiments()
    } ?: emptyList()

    @WorkerThread
    override fun getAvailableExperiments(): List<AvailableExperiment> = withCatchAll("getAvailableExperiments") {
        nimbusClient.getAvailableExperiments()
    } ?: emptyList()

    @AnyThread
    @VisibleForTesting(otherwise = VisibleForTesting.PRIVATE)
    internal fun getFeatureConfigVariablesJson(featureId: String): JSONObject? =
        @Suppress("TooGenericExceptionCaught")
        try {
            nimbusClient.getFeatureConfigVariables(featureId)?.let { JSONObject(it) }
        } catch (e: NimbusException.DatabaseNotReady) {
            NimbusHealth.cacheNotReadyForFeature.record(
                NimbusHealth.CacheNotReadyForFeatureExtra(
                    featureId = featureId,
                ),
            )
            null
        } catch (e: Throwable) {
            reportError("getFeatureConfigVariablesJson", e)
            null
        }

    private fun reportError(msg: String, e: Throwable) =
        @Suppress("TooGenericExceptionCaught")
        try {
            errorReporter("Nimbus Rust: $msg", e)
        } catch (e1: Throwable) {
            logger("Exception calling rust: $e")
            logger("Exception reporting the exception: $e1")
        }

    override fun getExperimentBranch(experimentId: String): String? = withCatchAll("getExperimentBranch") {
        nimbusClient.getExperimentBranch(experimentId)
    }

    override fun getVariables(featureId: String, recordExposureEvent: Boolean): Variables =
        getFeatureConfigVariablesJson(featureId)?.let { json ->
            if (recordExposureEvent) {
                recordExposureEvent(featureId)
            }
            JSONVariables(context, json)
        }
            ?: NullVariables.instance

    @WorkerThread
    override fun getExperimentBranches(experimentId: String): List<Branch>? = withCatchAll("getExperimentBranches") {
        nimbusClient.getExperimentBranches(experimentId)
    }

    // Method and apparatus to catch any uncaught exceptions
    @SuppressWarnings("TooGenericExceptionCaught")
    private fun <R> withCatchAll(method: String, thunk: () -> R) =
        try {
            thunk()
        } catch (e: NimbusException.DatabaseNotReady) {
            // NOOP
            null
        } catch (e: Throwable) {
            reportError(method, e)
            null
        }

    @WorkerThread
    @VisibleForTesting(otherwise = VisibleForTesting.PRIVATE)
    internal fun initializeOnThisThread() = withCatchAll("initialize") {
        nimbusClient.initialize()
        postEnrolmentCalculation()
    }

    override fun fetchExperiments() {
        fetchScope.launch {
            fetchExperimentsOnThisThread()
        }
    }

    override fun setFetchEnabled(enabled: Boolean) {
        fetchScope.launch {
            withCatchAll("setFetchEnabled") {
                nimbusClient.setFetchEnabled(enabled)
            }
        }
    }

    override fun isFetchEnabled() = withCatchAll("isFetchEnabled") {
        nimbusClient.isFetchEnabled()
    } ?: true

    @WorkerThread
    @VisibleForTesting(otherwise = VisibleForTesting.PRIVATE)
    internal fun fetchExperimentsOnThisThread() = withCatchAll("fetchExperiments") {
        try {
            NimbusHealth.fetchExperimentsTime.measure {
                nimbusClient.fetchExperiments()
            }
            updateObserver {
                it.onExperimentsFetched()
            }
        } catch (e: NimbusException.ClientException) {
            errorReporter("Error fetching experiments from endpoint", e)
        }
    }

    private fun updateObserver(updater: (NimbusInterface.Observer) -> Unit) {
        val observer = observer ?: return
        if (updateScope != null) {
            updateScope.launch {
                updater(observer)
            }
        } else {
            updater(observer)
        }
    }

    override fun applyPendingExperiments(): Job =
        dbScope.launch {
            withContext(NonCancellable) {
                applyPendingExperimentsOnThisThread()
            }
        }

    @WorkerThread
    @VisibleForTesting(otherwise = VisibleForTesting.PRIVATE)
    internal fun applyPendingExperimentsOnThisThread() = withCatchAll("applyPendingExperiments") {
        try {
            val events = NimbusHealth.applyPendingExperimentsTime.measure {
                nimbusClient.applyPendingExperiments()
            }
            recordExperimentTelemetryEvents(events)
            // Get the experiments to record in telemetry
            postEnrolmentCalculation()
        } catch (e: NimbusException.InvalidExperimentFormat) {
            reportError("Invalid experiment format", e)
        }
    }

    override fun applyLocalExperiments(@RawRes file: Int): Job =
        applyLocalExperiments { loadRawResource(file) }

    @VisibleForTesting(otherwise = VisibleForTesting.PRIVATE)
    fun applyLocalExperiments(getString: suspend () -> String): Job =
        dbScope.launch {
            val payload = try {
                getString()
            } catch (e: CancellationException) {
                // TODO consider reporting a glean event here.
                logger(e.stackTraceToString())
                null
            } catch (e: IOException) {
                logger(e.stackTraceToString())
                null
            }
            withContext(NonCancellable) {
                if (payload != null) {
                    setExperimentsLocallyOnThisThread(payload)
                    applyPendingExperimentsOnThisThread()
                } else {
                    initializeOnThisThread()
                }
            }
        }

    @WorkerThread
    private fun postEnrolmentCalculation() {
        nimbusClient.getActiveExperiments().let {
            recordExperimentTelemetry(it)
            updateObserver { observer ->
                observer.onUpdatesApplied(it)
            }
        }
    }

    override fun setExperimentsLocally(@RawRes file: Int) {
        dbScope.launch {
            withCatchAll("setExperimentsLocally") {
                loadRawResource(file)
            }?.let { payload ->
                setExperimentsLocallyOnThisThread(payload)
            }
        }
    }

    private fun loadRawResource(file: Int): String =
        context.resources.openRawResource(file).use {
            it.bufferedReader().readText()
        }

    override fun setExperimentsLocally(payload: String) {
        dbScope.launch {
            setExperimentsLocallyOnThisThread(payload)
        }
    }

    @WorkerThread
    @VisibleForTesting(otherwise = VisibleForTesting.PRIVATE)
    internal fun setExperimentsLocallyOnThisThread(payload: String) = withCatchAll("setExperimentsLocally") {
        nimbusClient.setExperimentsLocally(payload)
    }

    override fun resetEnrollmentsDatabase() =
        dbScope.launch {
            withCatchAll("resetEnrollments") {
                nimbusClient.resetEnrollments()
            }
        }

    @WorkerThread
    @VisibleForTesting(otherwise = VisibleForTesting.PRIVATE)
    internal fun setGlobalUserParticipationOnThisThread(active: Boolean) = withCatchAll("setGlobalUserParticipation") {
        val enrolmentChanges = nimbusClient.setGlobalUserParticipation(active)
        if (enrolmentChanges.isNotEmpty()) {
            recordExperimentTelemetryEvents(enrolmentChanges)
            postEnrolmentCalculation()
        }
    }

    override fun optOut(experimentId: String) {
        dbScope.launch {
            optOutOnThisThread(experimentId)
        }
    }

    @WorkerThread
    @VisibleForTesting(otherwise = VisibleForTesting.PRIVATE)
    internal fun optOutOnThisThread(experimentId: String) = withCatchAll("optOut") {
        nimbusClient.optOut(experimentId).also(::recordExperimentTelemetryEvents)
    }

    override fun resetTelemetryIdentifiers() {
        // The "dummy" field here is required for obscure reasons when generating code on desktop,
        // so we just automatically set it to a dummy value.
        val aru = AvailableRandomizationUnits(clientId = null, userId = null, nimbusId = null, dummy = 0)
        dbScope.launch {
            withCatchAll("resetTelemetryIdentifiers") {
                nimbusClient.resetTelemetryIdentifiers(aru).also { enrollmentChangeEvents ->
                    recordExperimentTelemetryEvents(enrollmentChangeEvents)
                }
            }
        }
    }

    override fun optInWithBranch(experimentId: String, branch: String) {
        dbScope.launch {
            withCatchAll("optIn") {
                nimbusClient.optInWithBranch(experimentId, branch).also(::recordExperimentTelemetryEvents)
            }
        }
    }

    override fun recordExposureEvent(featureId: String, experimentSlug: String?) {
        recordExposureOnThisThread(featureId, experimentSlug)
    }

    override fun recordMalformedConfiguration(featureId: String, partId: String) {
        recordMalformedConfigurationOnThisThread(featureId, partId)
    }

    @AnyThread
    override fun recordEvent(count: Long, eventId: String) {
        dbScope.launch {
            nimbusClient.recordEvent(eventId, count)
        }
    }

    override fun recordPastEvent(count: Long, eventId: String, secondsAgo: Long) =
        nimbusClient.recordPastEvent(eventId, secondsAgo, count)

    override fun advanceEventTime(bySeconds: Long) =
        nimbusClient.advanceEventTime(bySeconds)

    @WorkerThread
    override fun clearEvents() {
        dbScope.launch {
            nimbusClient.clearEvents()
        }
    }

    @AnyThread
    override fun dumpStateToLog() {
        nimbusClient.dumpStateToLog()
    }

    override fun createMessageHelper(additionalContext: JSONObject?): GleanPlumbMessageHelper =
        GleanPlumbMessageHelper(
            nimbusClient.createTargetingHelper(additionalContext),
            nimbusClient.createStringHelper(additionalContext),
        )

    @VisibleForTesting(otherwise = VisibleForTesting.PRIVATE)
    internal fun recordExperimentTelemetry(experiments: List<EnrolledExperiment>) {
        // Call Glean.setExperimentActive() for each active experiment.
        experiments.forEach { experiment ->
            // For now, we will just record the experiment id and the branch id. Once we can call
            // Glean from Rust, this will move to the nimbus-sdk Rust core.
            Glean.setExperimentActive(
                experiment.slug,
                experiment.branchSlug,
                mapOf("enrollmentId" to experiment.enrollmentId),
            )
        }
    }

    @VisibleForTesting(otherwise = VisibleForTesting.PRIVATE)
    internal fun recordExperimentTelemetryEvents(enrollmentChangeEvents: List<EnrollmentChangeEvent>) {
        enrollmentChangeEvents.forEach { event ->
            when (event.change) {
                EnrollmentChangeEventType.ENROLLMENT -> {
                    NimbusEvents.enrollment.record(
                        NimbusEvents.EnrollmentExtra(
                            experiment = event.experimentSlug,
                            branch = event.branchSlug,
                            enrollmentId = event.enrollmentId,
                        ),
                    )
                }
                EnrollmentChangeEventType.DISQUALIFICATION -> {
                    NimbusEvents.disqualification.record(
                        NimbusEvents.DisqualificationExtra(
                            experiment = event.experimentSlug,
                            branch = event.branchSlug,
                            enrollmentId = event.enrollmentId,
                        ),
                    )
                }
                EnrollmentChangeEventType.UNENROLLMENT -> {
                    NimbusEvents.unenrollment.record(
                        NimbusEvents.UnenrollmentExtra(
                            experiment = event.experimentSlug,
                            branch = event.branchSlug,
                            enrollmentId = event.enrollmentId,
                        ),
                    )
                }
                EnrollmentChangeEventType.ENROLL_FAILED -> {
                    NimbusEvents.enrollFailed.record(
                        NimbusEvents.EnrollFailedExtra(
                            experiment = event.experimentSlug,
                            branch = event.branchSlug,
                            reason = event.reason,
                        ),
                    )
                }
                EnrollmentChangeEventType.UNENROLL_FAILED -> {
                    NimbusEvents.unenrollFailed.record(
                        NimbusEvents.UnenrollFailedExtra(
                            experiment = event.experimentSlug,
                            reason = event.reason,
                        ),
                    )
                }
            }
        }
    }

    // The exposure event should be recorded when the expected treatment (or no-treatment, such as
    // for a "control" branch) is applied or shown to the user.
    // If the experiment slug is known, then use that to look up the enrollment.
    @VisibleForTesting(otherwise = VisibleForTesting.PRIVATE)
    @AnyThread
    internal fun recordExposureOnThisThread(featureId: String, experimentSlug: String? = null) {
        if (experimentSlug.isNullOrEmpty()) {
            recordExposureFromFeature(featureId)
        } else {
            recordExposureFromExperiment(featureId, experimentSlug)
        }
    }

    private fun recordExposureFromFeature(featureId: String) = withCatchAll("recordExposureFromFeature") {
        // First, we get the enrolled feature, if there is one, for this id.
        val enrollment = nimbusClient.getEnrollmentByFeature(featureId) ?: return@withCatchAll
        // If branch is null, this is a rollout, and we're not interested in recording
        // exposure for rollouts.
        val branch = enrollment.branch ?: return@withCatchAll
        // Finally, if we do have an experiment for the given featureId, we will record the
        // exposure event in Glean. This is to protect against accidentally recording an event
        // for an experiment without an active enrollment.
        NimbusEvents.exposure.record(
            NimbusEvents.ExposureExtra(
                experiment = enrollment.slug,
                branch = branch,
                featureId = featureId,
            ),
        )
    }

    private fun recordExposureFromExperiment(featureId: String, slug: String) = withCatchAll("recordExposureFromExperiment") {
        nimbusClient.getExperimentBranch(slug)?.let { branch ->
            NimbusEvents.exposure.record(
                NimbusEvents.ExposureExtra(
                    experiment = slug,
                    branch = branch,
                    featureId = featureId,
                ),
            )
        }
    }

    // The malformed feature event is recorded by app developers, if the configuration is
    // _semantically_ invalid or malformed.
    @VisibleForTesting(otherwise = VisibleForTesting.PRIVATE)
    @AnyThread
    internal fun recordMalformedConfigurationOnThisThread(featureId: String, partId: String) = withCatchAll("recordMalformedConfiguration") {
        // First, we get the enrolled feature, if there is one, for this id.
        val enrollment = nimbusClient.getEnrollmentByFeature(featureId)
        // If the enrollment is null, then that's the most serious: we've shipped invalid FML,
        // If the branch is null, then this is also serious: we've shipped an invalid rollout.
        NimbusEvents.malformedFeature.record(
            NimbusEvents.MalformedFeatureExtra(
                experiment = enrollment?.slug,
                branch = enrollment?.branch,
                featureId = featureId,
                partId = partId,
            ),
        )
    }

    @VisibleForTesting(otherwise = VisibleForTesting.PRIVATE)
    internal fun buildExperimentContext(context: Context, appInfo: NimbusAppInfo, deviceInfo: NimbusDeviceInfo): AppContext {
        val packageInfo: PackageInfo? = try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                context.packageManager.getPackageInfo(context.packageName, PackageManager.PackageInfoFlags.of(0))
            } else {
                @Suppress("DEPRECATION")
                context.packageManager.getPackageInfo(context.packageName, 0)
            }
        } catch (e: PackageManager.NameNotFoundException) {
            null
        }

        return AppContext(
            appId = context.packageName,
            appName = appInfo.appName,
            channel = appInfo.channel,
            androidSdkVersion = Build.VERSION.SDK_INT.toString(),
            appBuild = packageInfo?.let { PackageInfoCompat.getLongVersionCode(it).toString() },
            appVersion = packageInfo?.versionName,
            architecture = Build.SUPPORTED_ABIS[0],
            debugTag = null,
            deviceManufacturer = Build.MANUFACTURER,
            deviceModel = Build.MODEL,
            locale = deviceInfo.localeTag,
            os = "Android",
            osVersion = Build.VERSION.RELEASE,
            installationDate = packageInfo?.firstInstallTime,
            homeDirectory = context.applicationInfo?.dataDir,
            customTargetingAttributes = appInfo.customTargetingAttributes,
        )
    }
}
