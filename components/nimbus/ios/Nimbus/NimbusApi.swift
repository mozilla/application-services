/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation

/// This is the main experiments API, which is exposed through the global [Nimbus] object.
///
/// Application developers are encouraged to build against this API protocol, and use the `Nimbus.create` method
/// to create the correct implementation for them.
///
/// Feature developers configuring their features shoiuld use the methods in `NimbusFeatureConfiguration`.
/// These are safe to call from any thread. Developers building UI tools for the user or QA to modify experiment
/// enrollment will mostly use `NimbusUserConfiguration` methods. Application developers integrating
/// `Nimbus` into their app should use the methods in `NimbusStartup`.
///
public protocol NimbusApi: FeaturesInterface, NimbusStartup,
    NimbusUserConfiguration {}

public extension FeaturesInterface {
    /// Get the variables needed to configure the feature given by `featureId`.
    ///
    /// By default this sends an exposure event.
    ///
    /// - Parameters:
    ///     - featureId The string feature id that identifies to the feature under experiment.
    ///
    /// - Returns a `Variables` object used to configure the feature.
    func getVariables(featureId: String) -> Variables {
        return getVariables(featureId: featureId, sendExposureEvent: true)
    }
}

public protocol NimbusStartup {
    /// Open the database and populate the SDK so as make it usable by feature developers.
    ///
    /// This performs the minimum amount of I/O needed to ensure `getExperimentBranch()` is usable.
    ///
    /// It will not take in to consideration previously fetched experiments: `applyPendingExperiments()`
    /// is more suitable for that use case.
    ///
    /// This method uses the single threaded worker scope, so callers can safely sequence calls to
    /// `initialize` and `setExperimentsLocally`, `applyPendingExperiments`.
    ///
    func initialize()

    /// Fetches experiments from the RemoteSettings server.
    ///
    /// This is performed on a background thread.
    ///
    /// Notifies `.nimbusExperimentsFetched` to observers once the experiments has been fetched from the
    /// server.
    ///
    /// Notes:
    /// * this does not affect experiment enrollment, until `applyPendingExperiments` is called.
    /// * this will overwrite pending experiments previously fetched with this method, or set with
    ///  `setExperimentsLocally`.
    ///
    func fetchExperiments()

    /// Calculates the experiment enrollment from experiments from the last `fetchExperiments` or
    /// `setExperimentsLocally`, and then informs Glean of new experiment enrollment.
    ///
    /// Notifies `.nimbusExperimentsApplied` once enrollments are recalculated.
    ///
    func applyPendingExperiments()

    /// Set the experiments as the passed string, just as `fetchExperiments` gets the string from
    /// the server. Like `fetchExperiments`, this requires `applyPendingExperiments` to be called
    /// before enrollments are affected.
    ///
    /// The string should be in the same JSON format that is delivered from the server.
    ///
    /// This is performed on a background thread.
    ///
    /// - Parameter experimentsJson string representation of the JSON document in the same format
    ///             delivered by RemoteSettings.
    ///
    func setExperimentsLocally(_ experimentsJson: String)

    /// A utility method to load a file from resources and pass it to `setExperimentsLocally(String)`.
    ///
    /// - Parameter fileURL the URL of a JSON document in the app `Bundle`.
    ///
    func setExperimentsLocally(_ fileURL: URL)
}

public protocol NimbusUserConfiguration {
    /// Opt out of a specific experiment
    ///
    /// - Parameter experimentId The string id or "slug"  of the experiment for which to opt out of
    ///
    func optOut(_ experimentId: String)

    /// Opt in to a specific experiment with a particular branch.
    ///
    /// For data-science reasons: This should not be utilizable by the the user.
    ///
    /// - Parameters:
    ///    - experimentId The id or slug of the experiment to opt in
    ///    - branch The id or slug of the branch with which to enroll.
    ///
    func optIn(_ experimentId: String, branch: String)

    /// Call this when toggling user preferences about sending analytics.
    func resetTelemetryIdentifiers()

    /// Control the opt out for all experiments at once. This is likely a user action.
    ///
    var globalUserParticipation: Bool { get set }

    /// Get the list of currently enrolled experiments
    ///
    /// - Returns  A list of `EnrolledExperiment`s
    ///
    func getActiveExperiments() -> [EnrolledExperiment]

    /// For a given experiment id, returns the branches available.
    ///
    /// - Parameter experimentId the specifies the experiment.
    /// - Returns a list of one more branches for the given experiment, or `nil` if no such experiment exists.
    func getExperimentBranches(_ experimentId: String) -> [Branch]?

    /// Get the list of currently available experiments for the `appName` as specified in the `AppContext`.
    ///
    /// - Returns  A list of `AvailableExperiment`s
    ///
    func getAvailableExperiments() -> [AvailableExperiment]
}

/// Notifications emitted by the `NotificationCenter`.
///
public extension Notification.Name {
    static let nimbusExperimentsFetched = Notification.Name("nimbusExperimentsFetched")
    static let nimbusExperimentsApplied = Notification.Name("nimbusExperimentsApplied")
}

/// This struct is used during in the `create` method to point `Nimbus` at the given `RemoteSettings` server.
///
public struct NimbusServerSettings {
    public init(url: URL, collection: String = remoteSettingsCollection) {
        self.url = url
        self.collection = collection
    }

    public let url: URL
    public let collection: String
}

public let remoteSettingsCollection = "nimbus-mobile-experiments"

/// Name, channel and specific context of the app which should agree with what is specified in Experimenter.
/// The specifc context is there to capture any context that the SDK doesn't need to be explictly aware of.
///
public struct NimbusAppSettings {
    public init(appName: String, channel: String, customTargetingAttributes: [String: String] = [String: String]()) {
        self.appName = appName
        self.channel = channel
        self.customTargetingAttributes = customTargetingAttributes
    }

    public let appName: String
    public let channel: String
    public let customTargetingAttributes: [String: String]
}

/// This error reporter is passed to `Nimbus` and any errors that are caught are reported via this type.
///
public typealias NimbusErrorReporter = (Error) -> Void

/// `ExperimentBranch` is a copy of the `Branch` without the `FeatureConfig`.
public typealias Branch = ExperimentBranch
