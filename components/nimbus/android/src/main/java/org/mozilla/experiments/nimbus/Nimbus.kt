/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

@file:Suppress("TooManyFunctions")

package org.mozilla.experiments.nimbus

import android.content.Context
import android.content.pm.PackageInfo
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import androidx.annotation.AnyThread
import androidx.annotation.RawRes
import androidx.annotation.VisibleForTesting
import androidx.annotation.WorkerThread
import androidx.core.content.pm.PackageInfoCompat
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import mozilla.components.service.glean.Glean
import org.json.JSONObject
import org.mozilla.experiments.nimbus.GleanMetrics.NimbusEvents
import org.mozilla.experiments.nimbus.internal.AppContext
import org.mozilla.experiments.nimbus.internal.AvailableExperiment
import org.mozilla.experiments.nimbus.internal.AvailableRandomizationUnits
import org.mozilla.experiments.nimbus.internal.EnrolledExperiment
import org.mozilla.experiments.nimbus.internal.EnrollmentChangeEvent
import org.mozilla.experiments.nimbus.internal.EnrollmentChangeEventType
import org.mozilla.experiments.nimbus.internal.ExperimentBranch
import org.mozilla.experiments.nimbus.internal.NimbusException
import org.mozilla.experiments.nimbus.internal.NimbusClient
import org.mozilla.experiments.nimbus.internal.NimbusClientInterface
import org.mozilla.experiments.nimbus.internal.RemoteSettingsConfig
import java.io.File

private const val EXPERIMENT_COLLECTION_NAME = "nimbus-mobile-experiments"
private const val NIMBUS_DATA_DIR: String = "nimbus_data"

// Republish these classes from this package.
typealias Branch = ExperimentBranch
typealias AvailableExperiment = AvailableExperiment
typealias EnrolledExperiment = EnrolledExperiment

/**
 * This is the main experiments API, which is exposed through the global [Nimbus] object.
 */
interface NimbusInterface : FeaturesInterface {
    /**
     * Get the list of currently enrolled experiments
     *
     * @return A list of [EnrolledExperiment]s
     */
    fun getActiveExperiments(): List<EnrolledExperiment> = listOf()

    /**
     * Get the list of available experiments
     *
     * @return A list of [AvailableExperiment]s
     */
    fun getAvailableExperiments(): List<AvailableExperiment> = listOf()

    /**
     * Get the currently enrolled branch for the given experiment
     *
     * @param experimentId The string experiment-id or "slug" for which to retrieve the branch
     *
     * @return A String representing the branch-id or "slug"
     */
    @AnyThread
    fun getExperimentBranch(experimentId: String): String? = null

    /**
     * Get the list of experiment branches for the given experiment
     *
     * @param experimentId The string experiment-id or "slug" for which to retrieve the branch
     *
     * @return A list of [Branch]s
     */
    fun getExperimentBranches(experimentId: String): List<Branch>? = listOf()

    /**
     * Get the variables needed to configure the feature given by `featureId`.
     *
     * @param featureId The string feature id that identifies to the feature under experiment.
     *
     * @param recordExposureEvent Passing `true` to this parameter will record the exposure event
     *      automatically if the client is enrolled in an experiment for the given [featureId].
     *      Passing `false` here indicates that the application will manually record the exposure
     *      event by calling the `recordExposureEvent` function at the time of the exposure to the
     *      feature.
     *
     * See [recordExposureEvent] for more information on manually recording the event.
     *
     * @return a [Variables] object used to configure the feature.
     */
    @AnyThread
    override fun getVariables(featureId: String, recordExposureEvent: Boolean): Variables = NullVariables.instance

    /**
     * Open the database and populate the SDK so as make it usable by feature developers.
     *
     * This performs the minimum amount of I/O needed to ensure `getExperimentBranch()` is usable.
     *
     * It will not take in to consideration previously fetched experiments: `applyPendingExperiments()`
     * is more suitable for that use case.
     *
     * This method uses the single threaded worker scope, so callers can safely sequence calls to
     * `initialize` and `setExperimentsLocally`, `applyPendingExperiments`.
     */
    fun initialize() = Unit

    /**
     * Fetches experiments from the RemoteSettings server.
     *
     * This is performed on a background thread.
     *
     * Notifies `onExperimentsFetched` to observers once the experiments has been fetched from the
     * server.
     *
     * Notes:
     * * this does not affect experiment enrolment, until `applyPendingExperiments` is called.
     * * this will overwrite pending experiments previously fetched with this method, or set with
     *   `setExperimentsLocally`.
     */
    fun fetchExperiments() = Unit

    /**
     * Calculates the experiment enrolment from experiments from the last `fetchExperiments` or
     * `setExperimentsLocally`, and then informs Glean of new experiment enrolment.
     *
     * Notifies `onUpdatesApplied` once enrolments are recalculated.
     */
    fun applyPendingExperiments() = Unit

    /**
     * Set the experiments as the passed string, just as `fetchExperiments` gets the string from
     * the server. Like `fetchExperiments`, this requires `applyPendingExperiments` to be called
     * before enrolments are affected.
     *
     * The string should be in the same JSON format that is delivered from the server.
     *
     * This is performed on a background thread.
     */
    fun setExperimentsLocally(payload: String) = Unit

    /**
     * A utility method to load a file from resources and pass it to `setExperimentsLocally(String)`.
     */
    fun setExperimentsLocally(@RawRes file: Int) = Unit

    /**
     * Opt into a specific branch for the given experiment.
     *
     * @param experimentId The string experiment-id or "slug" for which to opt into
     * @param branch The string branch slug for which to opt into
     */
    fun optInWithBranch(experimentId: String, branch: String) = Unit

    /**
     * Opt out of a specific experiment
     *
     * @param experimentId The string experiment-id or "slug" for which to opt out of
     */
    fun optOut(experimentId: String) = Unit

    /**
     *  Reset internal state in response to application-level telemetry reset.
     *  Consumers should call this method when the user resets the telemetry state of the
     *  consuming application, such as by opting out of (or in to) submitting telemetry.
     */
    fun resetTelemetryIdentifiers() = Unit

    /**
     * Records the `exposure` event in telemetry.
     *
     * This is a manual function to accomplish the same purpose as passing `true` as the
     * `recordExposureEvent` property of the [getVariables] function. It is intended to be used
     * when requesting feature variables must occur at a different time than the actual user's
     * exposure to the feature within the app.
     *
     * Examples:
     * * If the [Variables] are needed at a different time than when the exposure to the feature
     *   actually happens, such as constructing a menu happening at a different time than the user
     *   seeing the menu.
     * * If [getVariables] is required to be called multiple times for the same feature and it is
     *   desired to only record the exposure once, such as if [getVariables] were called with every
     *   keystroke.
     *
     * In the case where the use of this function is required, then the [getVariables] function
     * should be called with `false` so that the exposure event is not recorded when the variables
     * are fetched.
     *
     * This function is safe to call even when there is no active experiment for the feature. The SDK
     * will ensure that an event is only recorded for active experiments.
     *
     * @param featureId string representing the id of the feature for which to record the exposure
     *     event.
     */
    override fun recordExposureEvent(featureId: String) = Unit

    /**
     * Control the opt out for all experiments at once. This is likely a user action.
     */
    var globalUserParticipation: Boolean
        get() = false
        set(_) = Unit

    /**
     * Interface to be implemented by classes that want to observe experiment updates
     */
    interface Observer {
        /**
         * Event to indicate that the experiments have been fetched from the endpoint
         */
        fun onExperimentsFetched() = Unit

        /**
         * Event to indicate that the experiment enrollments have been applied. Multiple calls to
         * get the active experiments will return the same value so this has limited usefulness for
         * most feature developers
         */
        fun onUpdatesApplied(updated: List<EnrolledExperiment>) = Unit
    }
}

/**
 * This class allows client apps to configure Nimbus to point to your own server.
 * Client app developers should set up their own Nimbus infrastructure, to avoid different
 * organizations running conflicting experiments or hitting servers with extra network traffic.
 */
data class NimbusServerSettings(
    val url: Uri,
    val collection: String = EXPERIMENT_COLLECTION_NAME
)

typealias ErrorReporter = (message: String, e: Throwable) -> Unit

private typealias LoggerFunction = (message: String) -> Unit

/**
 * This class represents the client application name and channel for filtering purposes
 */
data class NimbusAppInfo(
    /**
     * The app name, used for experiment filtering purposes so that only the intended application
     * is targeted for the experiment.
     *
     * Examples: "fenix", "focus".
     *
     * For Mozilla products, this is defined in the telemetry system. For more context on where the
     * app_name comes for Mozilla products from see:
     * https://probeinfo.telemetry.mozilla.org/v2/glean/app-listings
     * and
     * https://github.com/mozilla/probe-scraper/blob/master/repositories.yaml
     */
    val appName: String,
    /**
     * The app channel used for experiment filtering purposes, so that only the intended application
     * channel is targeted for the experiment.
     *
     * Examples: "nightly", "beta", "release"
     */
    val channel: String,
    /**
     * Application derived attributes measured by the application, but useful for targeting of experiments.
     *
     * Example: mapOf("userType": "casual", "isFirstTime": "true")
     */
    val customTargetingAttributes: Map<String, String> = mapOf()
)

/**
 * Small struct for info derived from the device itself.
 */
data class NimbusDeviceInfo(
    val localeTag: String
)

/**
 * Provide calling apps control how Nimbus fits into it.
 */
class NimbusDelegate(
    val dbScope: CoroutineScope,
    val fetchScope: CoroutineScope,
    val errorReporter: ErrorReporter,
    val logger: LoggerFunction
)

/**
 * A implementation of the [NimbusInterface] interface backed by the Nimbus SDK.
 */
@Suppress("LargeClass", "LongParameterList")
open class Nimbus(
    private val context: Context,
    appInfo: NimbusAppInfo,
    server: NimbusServerSettings?,
    deviceInfo: NimbusDeviceInfo,
    private val observer: NimbusInterface.Observer? = null,
    delegate: NimbusDelegate
) : NimbusInterface {
    // An I/O scope is used for reading or writing from the Nimbus's RKV database.
    private val dbScope: CoroutineScope = delegate.dbScope

    // An I/O scope is used for getting experiments from the network.
    private val fetchScope: CoroutineScope = delegate.fetchScope

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
        // Set the name of the native library so that we use
        // the appservices megazord for compiled code.
        System.setProperty(
            "uniffi.component.nimbus.libraryOverride",
            System.getProperty("mozilla.appservices.megazord.library", "megazord")
        )
        // Build a File object to represent the data directory for Nimbus data
        val dataDir = File(context.applicationInfo.dataDir, NIMBUS_DATA_DIR)

        // Build Nimbus AppContext object to pass into initialize
        val experimentContext = buildExperimentContext(context, appInfo, deviceInfo)

        // Initialize Nimbus
        val remoteSettingsConfig = server?.let {
            RemoteSettingsConfig(
                serverUrl = it.url.toString(),
                collectionName = it.collection
            )
        }

        nimbusClient = NimbusClient(
            experimentContext,
            dataDir.path,
            remoteSettingsConfig,
            // The "dummy" field here is required for obscure reasons when generating code on desktop,
            // so we just automatically set it to a dummy value.
            AvailableRandomizationUnits(clientId = null, dummy = 0)
        )
    }

    // This is currently not available from the main thread.
    // see https://jira.mozilla.com/browse/SDK-191
    @WorkerThread
    override fun getActiveExperiments(): List<EnrolledExperiment> = withCatchAll {
        nimbusClient.getActiveExperiments()
    } ?: emptyList()

    @WorkerThread
    override fun getAvailableExperiments(): List<AvailableExperiment> = withCatchAll {
        nimbusClient.getAvailableExperiments()
    } ?: emptyList()

    @AnyThread
    @VisibleForTesting(otherwise = VisibleForTesting.PRIVATE)
    internal fun getFeatureConfigVariablesJson(featureId: String) =
        withCatchAll {
            nimbusClient.getFeatureConfigVariables(featureId)?.let { JSONObject(it) }
        }

    override fun getExperimentBranch(experimentId: String): String? = withCatchAll {
        nimbusClient.getExperimentBranch(experimentId)
    }

    override fun getVariables(featureId: String, recordExposureEvent: Boolean): Variables =
        getFeatureConfigVariablesJson(featureId)?.let { json ->
            if (recordExposureEvent) {
                recordExposure(featureId)
            }
            JSONVariables(context, json)
        }
        ?: NullVariables.instance

    @WorkerThread
    override fun getExperimentBranches(experimentId: String): List<Branch>? = withCatchAll {
        nimbusClient.getExperimentBranches(experimentId)
    }

    // Method and apparatus to catch any uncaught exceptions
    @SuppressWarnings("TooGenericExceptionCaught")
    private fun <R> withCatchAll(thunk: () -> R) =
        try {
            thunk()
        } catch (e: Throwable) {
            if (e !is NimbusException.DatabaseNotReady) {
                try {
                    errorReporter("Error in Nimbus Rust", e)
                } catch (e1: Throwable) {
                    logger("Exception calling rust: $e")
                    logger("Exception reporting the exception: $e1")
                }
            }
            null
        }

    override fun initialize() {
        dbScope.launch {
            initializeOnThisThread()
        }
    }

    @WorkerThread
    @VisibleForTesting(otherwise = VisibleForTesting.PRIVATE)
    internal fun initializeOnThisThread() = withCatchAll {
        nimbusClient.initialize()
    }

    override fun fetchExperiments() {
        fetchScope.launch {
            fetchExperimentsOnThisThread()
        }
    }

    @WorkerThread
    @VisibleForTesting(otherwise = VisibleForTesting.PRIVATE)
    internal fun fetchExperimentsOnThisThread() = withCatchAll {
        try {
            nimbusClient.fetchExperiments()
            observer?.onExperimentsFetched()
        } catch (e: NimbusException.RequestException) {
            errorReporter("Error fetching experiments from endpoint", e)
        } catch (e: NimbusException.ResponseException) {
            errorReporter("Error fetching experiments from endpoint", e)
        }
    }

    override fun applyPendingExperiments() {
        dbScope.launch {
            applyPendingExperimentsOnThisThread()
        }
    }

    @WorkerThread
    @VisibleForTesting(otherwise = VisibleForTesting.PRIVATE)
    internal fun applyPendingExperimentsOnThisThread() = withCatchAll {
        try {
            nimbusClient.applyPendingExperiments().also(::recordExperimentTelemetryEvents)
            // Get the experiments to record in telemetry
            postEnrolmentCalculation()
        } catch (e: NimbusException.InvalidExperimentFormat) {
            errorReporter("Invalid experiment format", e)
        }
    }

    @WorkerThread
    private fun postEnrolmentCalculation() {
        nimbusClient.getActiveExperiments().let {
            recordExperimentTelemetry(it)
            observer?.onUpdatesApplied(it)
        }
    }

    override fun setExperimentsLocally(@RawRes file: Int) {
        dbScope.launch {
            withCatchAll {
                context.resources.openRawResource(file).use {
                    it.bufferedReader().readText()
                }
            }?.let { payload ->
                setExperimentsLocallyOnThisThread(payload)
            }
        }
    }

    override fun setExperimentsLocally(payload: String) {
        dbScope.launch {
            setExperimentsLocallyOnThisThread(payload)
        }
    }

    @WorkerThread
    @VisibleForTesting(otherwise = VisibleForTesting.PRIVATE)
    internal fun setExperimentsLocallyOnThisThread(payload: String) = withCatchAll {
        nimbusClient.setExperimentsLocally(payload)
    }

    @WorkerThread
    @VisibleForTesting(otherwise = VisibleForTesting.PRIVATE)
    internal fun setGlobalUserParticipationOnThisThread(active: Boolean) = withCatchAll {
        val enrolmentChanges = nimbusClient.setGlobalUserParticipation(active)
        if (enrolmentChanges.isNotEmpty()) {
            recordExperimentTelemetryEvents(enrolmentChanges)
            postEnrolmentCalculation()
        }
    }

    override fun optOut(experimentId: String) {
        dbScope.launch {
            withCatchAll {
                optOutOnThisThread(experimentId)
            }
        }
    }

    @WorkerThread
    @VisibleForTesting(otherwise = VisibleForTesting.PRIVATE)
    internal fun optOutOnThisThread(experimentId: String) {
        nimbusClient.optOut(experimentId).also(::recordExperimentTelemetryEvents)
    }

    override fun resetTelemetryIdentifiers() {
        // The "dummy" field here is required for obscure reasons when generating code on desktop,
        // so we just automatically set it to a dummy value.
        val aru = AvailableRandomizationUnits(clientId = null, dummy = 0)
        dbScope.launch {
            withCatchAll {
                nimbusClient.resetTelemetryIdentifiers(aru).also { enrollmentChangeEvents ->
                    recordExperimentTelemetryEvents(enrollmentChangeEvents)
                }
            }
        }
    }

    override fun optInWithBranch(experimentId: String, branch: String) {
        dbScope.launch {
            withCatchAll {
                nimbusClient.optInWithBranch(experimentId, branch).also(::recordExperimentTelemetryEvents)
            }
        }
    }

    override fun recordExposureEvent(featureId: String) {
        recordExposure(featureId)
    }

    @VisibleForTesting(otherwise = VisibleForTesting.PRIVATE)
    internal fun recordExperimentTelemetry(experiments: List<EnrolledExperiment>) {
        // Call Glean.setExperimentActive() for each active experiment.
        experiments.forEach { experiment ->
            // For now, we will just record the experiment id and the branch id. Once we can call
            // Glean from Rust, this will move to the nimbus-sdk Rust core.
            Glean.setExperimentActive(
                experiment.slug,
                experiment.branchSlug,
                mapOf("enrollmentId" to experiment.enrollmentId)
            )
        }
    }

    @VisibleForTesting(otherwise = VisibleForTesting.PRIVATE)
    internal fun recordExperimentTelemetryEvents(enrollmentChangeEvents: List<EnrollmentChangeEvent>) {
        enrollmentChangeEvents.forEach { event ->
            when (event.change) {
                EnrollmentChangeEventType.ENROLLMENT -> {
                    NimbusEvents.enrollment.record(NimbusEvents.EnrollmentExtra(
                        experiment = event.experimentSlug,
                        branch = event.branchSlug,
                        enrollmentId = event.enrollmentId
                    ))
                }
                EnrollmentChangeEventType.DISQUALIFICATION -> {
                    NimbusEvents.disqualification.record(NimbusEvents.DisqualificationExtra(
                        experiment = event.experimentSlug,
                        branch = event.branchSlug,
                        enrollmentId = event.enrollmentId
                    ))
                }
                EnrollmentChangeEventType.UNENROLLMENT -> {
                    NimbusEvents.unenrollment.record(NimbusEvents.UnenrollmentExtra(
                        experiment = event.experimentSlug,
                        branch = event.branchSlug,
                        enrollmentId = event.enrollmentId
                    ))
                }
            }
        }
    }

    @VisibleForTesting(otherwise = VisibleForTesting.PRIVATE)
    internal fun recordExposure(featureId: String) {
        dbScope.launch {
            recordExposureOnThisThread(featureId)
        }
    }

    // The exposure event should be recorded when the expected treatment (or no-treatment, such as
    // for a "control" branch) is applied or shown to the user.
    @VisibleForTesting(otherwise = VisibleForTesting.PRIVATE)
    @WorkerThread
    internal fun recordExposureOnThisThread(featureId: String) = withCatchAll {
        val activeExperiments = getActiveExperiments()
        activeExperiments.find { it.featureIds.contains(featureId) }?.also { experiment ->
            NimbusEvents.exposure.record(NimbusEvents.ExposureExtra(
                experiment = experiment.slug,
                branch = experiment.branchSlug,
                enrollmentId = experiment.enrollmentId
            ))
        }
    }

    @VisibleForTesting(otherwise = VisibleForTesting.PRIVATE)
    internal fun buildExperimentContext(context: Context, appInfo: NimbusAppInfo, deviceInfo: NimbusDeviceInfo): AppContext {
        val packageInfo: PackageInfo? = try {
            context.packageManager.getPackageInfo(
                context.packageName, 0
            )
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
            customTargetingAttributes = appInfo.customTargetingAttributes)
    }
}
