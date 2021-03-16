/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation

/**
 * This is the main experiments API, which is exposed through the global [Nimbus] object.
 *
 * Application developers are encouraged to build against this API protocol, and use the `Nimbus.create` method
 * to create the correct implementation for them.
 *
 * Feature developers configuring their features shoiuld use the methods in `NimbusFeatureConfiguration`. These are safe to call from any thread.
 * Developers building UI tools for the user or QA to modify experiment enrollment will mostly use `NimbusUserConfiguration` methods.
 * Application developers integrating `Nimbus` into their app should use the methods in `NimbusStartup`.
 */
public protocol NimbusApi: NimbusStartup, NimbusFeatureConfiguration, NimbusUserConfiguration {}

public protocol NimbusFeatureConfiguration {
    /**
     * Get the currently enrolled branch for the given experiment
     *
     * @param featureId The string feature id that applies to the feature under experiment.
     *
     * @return A String representing the branch-id or "slug"; or `nil` if not enrolled in this experiment.
     */
    func getExperimentBranch(featureId: String) -> String?
}

public protocol NimbusStartup {
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
    func initialize()

    /**
     * Fetches experiments from the RemoteSettings server.
     *
     * This is performed on a background thread.
     *
     * Notifies `.nimbusExperimentsFetched` to observers once the experiments has been fetched from the
     * server.
     *
     * Notes:
     * * this does not affect experiment enrolment, until `applyPendingExperiments` is called.
     * * this will overwrite pending experiments previously fetched with this method, or set with
     *   `setExperimentsLocally`.
     */
    func fetchExperiments()

    /**
     * Calculates the experiment enrolment from experiments from the last `fetchExperiments` or
     * `setExperimentsLocally`, and then informs Glean of new experiment enrolment.
     *
     * Notifies `.nimbusExperimentsApplied` once enrolments are recalculated.
     */
    func applyPendingExperiments()

    /**
     * Set the experiments as the passed string, just as `fetchExperiments` gets the string from
     * the server. Like `fetchExperiments`, this requires `applyPendingExperiments` to be called
     * before enrolments are affected.
     *
     * The string should be in the same JSON format that is delivered from the server.
     *
     * This is performed on a background thread.
     */
    func setExperimentsLocally(_ experimentsJson: String)

    /**
     * A utility method to load a file from resources and pass it to `setExperimentsLocally(String)`.
     */
    func setExperimentsLocally(_ fileURL: URL)
}

public protocol NimbusUserConfiguration {
    /**
     * Opt out of a specific experiment
     *
     * - Paramaters
     *         - experimentId The string experiment-id or "slug" for which to opt out of
     */
    func optOut(_ experimentId: String)

    /**
     * Opt in to a specific experiment with a particular branch.
     *
     * For data-science reasons: This should not be utilizable by the the user.
     */
    func optIn(_ experimentId: String, branch: String)

    func resetTelemetryIdentifiers(_ identifiers: AvailableRandomizationUnits)

    /**
     * Control the opt out for all experiments at once. This is likely a user action.
     */
    var globalUserParticipation: Bool { get set }

    /**
     * Get the list of currently enrolled experiments
     *
     * @return A list of [EnrolledExperiment]s
     */
    func getActiveExperiments() -> [EnrolledExperiment]
}

/**
 * Notifications emitted by the `NotificationCenter`.
 */
public extension Notification.Name {
    static let nimbusExperimentsFetched = Notification.Name("nimbusExperimentsFetched")
    static let nimbusExperimentsApplied = Notification.Name("nimbusExperimentsApplied")
}

/**
 * This struct is used during in the `create` method to point `Nimbus` at the given `RemoteSettings` server.
 */
public struct NimbusServerSettings {
    let url: URL
}

/**
 * Name and channel of the app, which should agree with what is specified in Experimenter.
 */
public struct NimbusAppSettings {
    let appName: String
    let channel: String
}

/**
 * This error reporter is passed to `Nimbus` and any errors that are caught are reported via this type.
 */
public typealias NimbusErrorReporter = (Error) -> Void
