/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.experiments.nimbus

import android.annotation.TargetApi
import android.content.Context
import android.os.Build
import androidx.annotation.AnyThread
import androidx.annotation.RawRes
import kotlinx.coroutines.Job
import org.mozilla.experiments.nimbus.GleanMetrics.NimbusEvents
import org.mozilla.experiments.nimbus.internal.AvailableExperiment
import org.mozilla.experiments.nimbus.internal.EnrolledExperiment
import org.mozilla.experiments.nimbus.internal.EnrollmentChangeEvent
import org.mozilla.experiments.nimbus.internal.ExperimentBranch
import org.mozilla.experiments.nimbus.internal.GeckoPrefState
import org.mozilla.experiments.nimbus.internal.PrefUnenrollReason
import java.time.Duration
import java.util.concurrent.TimeUnit

// Republish these classes from this package.
typealias Branch = ExperimentBranch
typealias AvailableExperiment = AvailableExperiment
typealias EnrolledExperiment = EnrolledExperiment

/**
 * This is the main experiments API, which is exposed through the global [Nimbus] object.
 */
interface NimbusInterface : FeaturesInterface, NimbusMessagingInterface, NimbusEventStore {

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
     * Enable or disable fetching of experiments.
     *
     * This is performed on a background thread.
     *
     * This is only used during QA of the app, and not meant for application developers.
     * Application developers should allow users to opt out with [globalUserParticipation]
     * instead.
     */
    fun setFetchEnabled(enabled: Boolean) = Unit

    /**
     * The complement for [setFetchEnabled].
     *
     * This is only used during QA of the app, and not meant for application developers.
     * Application developers should allow users to opt out with [globalUserParticipation]
     * instead.
     */
    fun isFetchEnabled(): Boolean = true

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
     * Testing method to reset the enrollments and experiments database back to its initial state.
     */
    fun resetEnrollmentsDatabase(): Job = Job()

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
     * Unenroll from experiments that relate to a particular Gecko pref
     *
     * @param geckoPrefState The Gecko pref state for which experiments should be unenrolled
     * @param prefUnenrollReason The reason we are unenrolling from the experiments
     * @return Returns a list of EnrollmentChangeEvents
     */
    fun unenrollForGeckoPref(geckoPrefState: GeckoPrefState, prefUnenrollReason: PrefUnenrollReason): List<EnrollmentChangeEvent> = listOf()

    /**
     *  Reset internal state in response to application-level telemetry reset.
     *  Consumers should call this method when the user resets the telemetry state of the
     *  consuming application, such as by opting out of (or in to) submitting telemetry.
     */
    fun resetTelemetryIdentifiers() = Unit

    /**
     * Control the opt out for all experiments at once. This is likely a user action.
     */
    var globalUserParticipation: Boolean
        get() = false
        set(_) = Unit

    override val events: NimbusEventStore
        get() = this

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

interface NimbusEventStore {
    /**
     * Records an event to the Nimbus event store.
     *
     * The method obtains the event counters for the `eventId` that is passed in, advances them if
     * needed, then increments the counts by 1. If an event counter does not exist for the `eventId`,
     * one will be created.
     *
     * @param eventId string representing the id of the event which should be recorded.
     */
    fun recordEvent(count: Long = 1, eventId: String) = Unit

    /**
     * Convenience method for [recordEvent].
     *
     * This method is discouraged, and will be removed after usage has been migrated to
     * the preferred [recordEvent].
     *
     * @see [recordEvent]
     */
    fun recordEvent(eventId: String) =
        recordEvent(1, eventId)

    /**
     * Records an event as if it were emitted in the past.
     *
     * This method is only likely useful during testing, and so is by design synchronous.
     *
     * @param count the number of events seen just now. This is usually 1.
     * @param eventId string representing the id of the event which should be recorded.
     * @param durationAgo the duration subtracted from now when the event are said to have happened.
     * @throws NimbusError if durationAgo is negative.
     */
    fun recordPastEvent(count: Long = 1, eventId: String, secondsAgo: Long) = Unit

    /**
     * Convenience method for [recordPastEvent].
     *
     * @see [recordPastEvent]
     */
    @TargetApi(Build.VERSION_CODES.O)
    fun recordPastEvent(count: Long = 1, eventId: String, durationAgo: Duration) =
        recordPastEvent(count, eventId, durationAgo.seconds)

    /**
     * Convenience method for [recordPastEvent].
     *
     * @see [recordPastEvent]
     */
    fun recordPastEvent(count: Long = 1, eventId: String, timeAgo: Long, timeUnit: TimeUnit) =
        recordPastEvent(count, eventId, timeUnit.toSeconds(timeAgo))

    /**
     * Advance the time of the event store into the future.
     *
     * This is not needed for normal operation, but is especially useful for testing queries,
     * without having to wait for actual time to pass.
     *
     * @param bySeconds the number of seconds to advance into the future. Must be positive.
     * @throws NimbusError is [bySeconds] is negative.
     */
    fun advanceEventTime(bySeconds: Long) = Unit

    /**
     * Convenience method for [advanceEventTime]
     *
     * @see [advanceEventTime]
     */
    @TargetApi(Build.VERSION_CODES.O)
    fun advanceEventTime(byDuration: Duration) =
        advanceEventTime(byDuration.seconds)

    /**
     * Convenience method for [advanceEventTime]
     *
     * @see [advanceEventTime]
     */
    fun advanceEventTime(byTime: Long, unit: TimeUnit) =
        advanceEventTime(unit.toSeconds(byTime))

    /**
     * Clears the Nimbus event store.
     *
     * This should only be used in testing or cases where the previous event store is no longer viable.
     */
    fun clearEvents() = Unit

    /**
     * Dump the state of the Nimbus SDK to logcat.
     *
     * This is only useful for testing.
     */
    fun dumpStateToLog() = Unit

    /**
     * Record the Nimbus `is_ready` event a number of times equal to the `count` variable.
     */
    fun recordIsReady(count: Int) {
        @Suppress("unused")
        for (i in 1..count) {
            NimbusEvents.isReady.record()
        }
    }
}

class NullNimbus(override val context: Context) : NimbusInterface
