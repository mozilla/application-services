/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.experiments.nimbus

import android.content.Context
import androidx.annotation.AnyThread
import androidx.annotation.RawRes
import kotlinx.coroutines.Job
import org.mozilla.experiments.nimbus.internal.AvailableExperiment
import org.mozilla.experiments.nimbus.internal.EnrolledExperiment
import org.mozilla.experiments.nimbus.internal.ExperimentBranch

// Republish these classes from this package.
typealias Branch = ExperimentBranch
typealias AvailableExperiment = AvailableExperiment
typealias EnrolledExperiment = EnrolledExperiment

/**
 * This is the main experiments API, which is exposed through the global [Nimbus] object.
 */
interface NimbusInterface : FeaturesInterface, GleanPlumbInterface {

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
    override fun getVariables(featureId: String, recordExposureEvent: Boolean): Variables =
        NullVariables.instance

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
    fun applyPendingExperiments(): Job = Job()

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
     * This is functionally equivalent to a sequence of {setExperimentsLocally} the
     * {applyPendingExperiments}.
     *
     * Following completion of the returned job, the SDK's Feature API is ready to be used. If
     * cancelled, the SDK will still prepare the SDK for safe use.
     *
     * Most apps will not need to call this method directly, as it is called on first run
     * as part of {initialize}.
     *
     * @param a `raw` resource identifier resolving to a JSON file downloaded from RemoteSettings
     *       at build time.
     * @return a Job. This may be cancelled, but only the loading from the resource will be cancelled.
     *      If this is cancelled, then {initialize} is called, which copies the database in to an
     *      in memory cache.
     */
    fun applyLocalExperiments(@RawRes file: Int): Job = Job()

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
     * Records an event to the Nimbus event store.
     *
     * The method obtains the event counters for the `eventId` that is passed in, advances them if
     * needed, then increments the counts by 1. If an event counter does not exist for the `eventId`,
     * one will be created.
     *
     * @param eventId string representing the id of the event which should be recorded.
     */
    override fun recordEvent(eventId: String) = Unit

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

class NullNimbus(override val context: Context) : NimbusInterface
