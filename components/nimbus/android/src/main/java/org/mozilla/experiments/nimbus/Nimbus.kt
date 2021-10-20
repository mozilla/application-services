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
import androidx.annotation.VisibleForTesting
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
import org.mozilla.experiments.nimbus.internal.NimbusClient
import org.mozilla.experiments.nimbus.internal.NimbusClientDecorator
import org.mozilla.experiments.nimbus.internal.NimbusClientInterface
import org.mozilla.experiments.nimbus.internal.NimbusException
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
interface NimbusInterface {
    fun getVariables(featureId: String, recordExposureEvent: Boolean = true): Variables =
        NullVariables.instance

    fun recordExposureEvent(featureId: String) = Unit

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

class NimbusDecorator(
    private val delegate: NimbusDelegate,
    private val observer: NimbusInterface.Observer? = null
) : NimbusClientDecorator<NimbusClient> {
    override fun <ReturnType> withCatchAll(obj: NimbusClient, thunk: () -> ReturnType) = try {
        thunk()
    } catch (e: Throwable) {
        if (e !is NimbusException.DatabaseNotReady) {
            try {
                delegate.errorReporter("Error in Nimbus Rust", e)
            } catch (e1: Throwable) {
                delegate.logger("Exception calling rust: $e")
                delegate.logger("Exception reporting the exception: $e1")
            }
        }
        null
    }

    override fun <ReturnType> onNetworkThread(obj: NimbusClient, thunk: () -> ReturnType) {
        delegate.fetchScope.launch {
            withCatchAll(obj, thunk)
            observer?.onExperimentsFetched()
        }
    }

    override fun <ReturnType> onDbThread(obj: NimbusClient, thunk: () -> ReturnType) {
        delegate.dbScope.launch {
            withCatchAll(obj, thunk)
        }
    }

    override fun <ReturnType> onEnrollmentChanges(obj: NimbusClient, thunk: () -> ReturnType) {
        onDbThread(obj) {
            withCatchAll(obj) {
                val result = thunk()
                if (result is List<*>) {
                    val events = result.filterIsInstance<EnrollmentChangeEvent>()
                    recordExperimentTelemetryEvents(events)
                }
                obj.getActiveExperiments()?.let {
                    recordExperimentTelemetry(it)
                    observer?.onUpdatesApplied(it)
                }
            }
        }
    }

    fun recordExperimentTelemetryEvents(enrollmentChangeEvents: List<EnrollmentChangeEvent>) {
        enrollmentChangeEvents.forEach { event ->
            when (event.change) {
                EnrollmentChangeEventType.ENROLLMENT -> {
                    NimbusEvents.enrollment.record(
                        NimbusEvents.EnrollmentExtra(
                            experiment = event.experimentSlug,
                            branch = event.branchSlug,
                            enrollmentId = event.enrollmentId
                        )
                    )
                }
                EnrollmentChangeEventType.DISQUALIFICATION -> {
                    NimbusEvents.disqualification.record(
                        NimbusEvents.DisqualificationExtra(
                            experiment = event.experimentSlug,
                            branch = event.branchSlug,
                            enrollmentId = event.enrollmentId
                        )
                    )
                }
                EnrollmentChangeEventType.UNENROLLMENT -> {
                    NimbusEvents.unenrollment.record(
                        NimbusEvents.UnenrollmentExtra(
                            experiment = event.experimentSlug,
                            branch = event.branchSlug,
                            enrollmentId = event.enrollmentId
                        )
                    )
                }
            }
        }
    }

    fun recordExperimentTelemetry(experiments: List<EnrolledExperiment>) {
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
}

/**
 * A implementation of the [NimbusInterface] interface backed by the Nimbus SDK.
 */
class Nimbus private constructor(private val context: Context, nimbusClient: NimbusClient) :
    NimbusInterface, NimbusClientInterface by nimbusClient {
    constructor(
        context: Context,
        decorator: NimbusClientDecorator<NimbusClient>,
        appInfo: NimbusAppInfo,
        server: NimbusServerSettings?,
        deviceInfo: NimbusDeviceInfo
    ) : this(
        context,
        NimbusClient(
            nimbusClientDecorator = decorator,
            appCtx = buildExperimentContext(context, appInfo, deviceInfo),
            dbpath = File(context.applicationInfo.dataDir, NIMBUS_DATA_DIR).path,
            remoteSettingsConfig = server?.let {
                RemoteSettingsConfig(
                    serverUrl = it.url.toString(),
                    collectionName = it.collection
                )
            },
            availableRandomizationUnits = AvailableRandomizationUnits(clientId = null, dummy = 0)
        )
    )

    constructor(
        context: Context,
        appInfo: NimbusAppInfo,
        server: NimbusServerSettings?,
        deviceInfo: NimbusDeviceInfo,
        observer: NimbusInterface.Observer? = null,
        delegate: NimbusDelegate
    ) : this(
        context,
        NimbusDecorator(delegate, observer),
        appInfo,
        server,
        deviceInfo
    )

    @AnyThread
    @VisibleForTesting(otherwise = VisibleForTesting.PRIVATE)
    internal fun getFeatureConfigVariablesJson(featureId: String) =
        getFeatureConfigVariables(featureId)?.let { JSONObject(it) }

    override fun getVariables(featureId: String, recordExposureEvent: Boolean): Variables =
        getFeatureConfigVariablesJson(featureId)?.let { json ->
            if (recordExposureEvent) {
                recordExposureEvent(featureId)
            }
            JSONVariables(context, json)
        }
            ?: NullVariables.instance

    override fun recordExposureEvent(featureId: String) {
        val activeExperiments = getActiveExperiments()
        activeExperiments?.find { it.featureIds.contains(featureId) }?.also { experiment ->
            NimbusEvents.exposure.record(
                NimbusEvents.ExposureExtra(
                    experiment = experiment.slug,
                    branch = experiment.branchSlug,
                    enrollmentId = experiment.enrollmentId
                )
            )
        }
    }

    companion object {
        init {
            System.setProperty(
                "uniffi.component.nimbus.libraryOverride",
                System.getProperty("mozilla.appservices.megazord.library", "megazord")
            )
        }

        internal fun buildExperimentContext(
            context: Context,
            appInfo: NimbusAppInfo,
            deviceInfo: NimbusDeviceInfo
        ): AppContext {
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
                customTargetingAttributes = appInfo.customTargetingAttributes
            )
        }
    }
}
