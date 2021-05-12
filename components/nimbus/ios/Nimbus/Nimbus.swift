/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation

public class Nimbus: NimbusApi {
    private let nimbusClient: NimbusClientProtocol

    private let errorReporter: NimbusErrorReporter

    lazy var fetchQueue: OperationQueue = {
        var queue = OperationQueue()
        queue.name = "Nimbus fetch queue"
        queue.maxConcurrentOperationCount = 1
        return queue
    }()

    lazy var dbQueue: OperationQueue = {
        var queue = OperationQueue()
        queue.name = "Nimbus database queue"
        queue.maxConcurrentOperationCount = 1
        return queue
    }()

    internal init(nimbusClient: NimbusClientProtocol, errorReporter: @escaping NimbusErrorReporter) {
        self.errorReporter = errorReporter
        self.nimbusClient = nimbusClient
    }
}

private extension Nimbus {
    func catchAll<T>(_ thunk: () throws -> T?) -> T? {
        do {
            return try thunk()
        } catch {
            errorReporter(error)
            return nil
        }
    }

    func catchAll(_ queue: OperationQueue, thunk: @escaping () throws -> Void) {
        queue.addOperation {
            self.catchAll(thunk)
        }
    }
}

// Glean integration
internal extension Nimbus {
    func recordExposure(experimentId: String) {
        let activeExperiments = getActiveExperiments()
        if let experiment = activeExperiments.first(where: { $0.slug == experimentId }) {
            GleanMetrics.NimbusEvents.exposure.record(extra: [
                .experiment: experiment.slug,
                .branch: experiment.branchSlug,
                .enrollmentId: experiment.enrollmentId,
            ])
        }
    }

    func postEnrollmentCalculation(_ events: [EnrollmentChangeEvent]) {
        // We need to update the experiment enrollment annotations in Glean
        // regardless of whether we recieved any events. Calling the
        // `setExperimentActive` function multiple times with the same
        // experiment id is safe so nothing bad should happen in case we do.
        let experiments = getActiveExperiments()
        recordExperimentTelemetry(experiments)

        // Record enrollment change events, if any
        if !events.isEmpty {
            recordExperimentEvents(events)

            // We are only notifying observers when we have enrollment
            // change events, to make the observer less chatty.
            notifyOnExperimentsApplied(experiments)
        }
    }

    func recordExperimentTelemetry(_ experiments: [EnrolledExperiment]) {
        for experiment in experiments {
            Glean.shared.setExperimentActive(
                experimentId: experiment.slug,
                branch: experiment.branchSlug,
                extra: ["enrollmentId": experiment.enrollmentId]
            )
        }
    }

    func recordExperimentEvents(_ events: [EnrollmentChangeEvent]) {
        for event in events {
            switch event.change {
            case .enrollment:
                GleanMetrics.NimbusEvents.enrollment.record(extra: [
                    .experiment: event.experimentSlug,
                    .branch: event.branchSlug,
                    .enrollmentId: event.enrollmentId,
                ])
            case .disqualification:
                GleanMetrics.NimbusEvents.disqualification.record(extra: [
                    .experiment: event.experimentSlug,
                    .branch: event.branchSlug,
                    .enrollmentId: event.enrollmentId,
                ])
            case .unenrollment:
                GleanMetrics.NimbusEvents.unenrollment.record(extra: [
                    .experiment: event.experimentSlug,
                    .branch: event.branchSlug,
                    .enrollmentId: event.enrollmentId,
                ])
            }
        }
    }
}

private extension Nimbus {
    func notifyOnExperimentsFetched() {
        NotificationCenter.default.post(name: .nimbusExperimentsFetched, object: nil)
    }

    func notifyOnExperimentsApplied(_ experiments: [EnrolledExperiment]) {
        NotificationCenter.default.post(name: .nimbusExperimentsApplied, object: experiments)
    }
}

/*
 * Methods split out onto a separate internal extension for testing purposes.
 */
internal extension Nimbus {
    func setGlobalUserParticipationOnThisThread(_ value: Bool) throws {
        let changes = try nimbusClient.setGlobalUserParticipation(optIn: value)
        postEnrollmentCalculation(changes)
    }

    func initializeOnThisThread() throws {
        try nimbusClient.initialize()
    }

    func fetchExperimentsOnThisThread() throws {
        try nimbusClient.fetchExperiments()
    }

    func applyPendingExperimentsOnThisThread() throws {
        let changes = try nimbusClient.applyPendingExperiments()
        postEnrollmentCalculation(changes)
    }

    func setExperimentsLocallyOnThisThread(_ experimentsJson: String) throws {
        try nimbusClient.setExperimentsLocally(experimentsJson: experimentsJson)
    }

    func optOutOnThisThread(_ experimentId: String) throws {
        let changes = try nimbusClient.optOut(experimentSlug: experimentId)
        postEnrollmentCalculation(changes)
    }

    func optInOnThisThread(_ experimentId: String, branch: String) throws {
        let changes = try nimbusClient.optInWithBranch(experimentSlug: experimentId, branch: branch)
        postEnrollmentCalculation(changes)
    }

    func resetTelemetryIdentifiersOnThisThread(_ identifiers: AvailableRandomizationUnits) throws {
        let changes = try nimbusClient.resetTelemetryIdentifiers(newRandomizationUnits: identifiers)
        postEnrollmentCalculation(changes)
    }
}

extension Nimbus: NimbusFeatureConfiguration {
    public func getExperimentBranch(featureId: String) -> String? {
        return catchAll {
            try nimbusClient.getExperimentBranch(id: featureId)
        }
    }

    internal func getFeatureConfigVariablesJson(featureId: String) -> [String: Any]? {
        return catchAll {
            if let string = try nimbusClient.getFeatureConfigVariables(featureId: featureId),
                   let data = string.data(using: .utf8) {
                return try? JSONSerialization.jsonObject(with: data, options: []) as? [String: Any]
            } else {
                return nil
            }
        }
    }
}

extension Nimbus: NimbusUserConfiguration {
    public var globalUserParticipation: Bool {
        get {
            catchAll { try nimbusClient.getGlobalUserParticipation() } ?? false
        }
        set {
            catchAll(dbQueue) {
                try self.setGlobalUserParticipationOnThisThread(newValue)
            }
        }
    }

    public func getActiveExperiments() -> [EnrolledExperiment] {
        return catchAll {
            try nimbusClient.getActiveExperiments()
        } ?? []
    }

    public func getAvailableExperiments() -> [AvailableExperiment] {
        return catchAll {
            try nimbusClient.getAvailableExperiments()
        } ?? []
    }

    public func getExperimentBranches(_ experimentId: String) -> [Branch]? {
        return catchAll {
            try nimbusClient.getExperimentBranches(experimentSlug: experimentId)
        }
    }

    public func optOut(_ experimentId: String) {
        catchAll(dbQueue) {
            try self.optOutOnThisThread(experimentId)
        }
    }

    public func optIn(_ experimentId: String, branch: String) {
        catchAll(dbQueue) {
            try self.optInOnThisThread(experimentId, branch: branch)
        }
    }

    public func resetTelemetryIdentifiers() {
        catchAll(dbQueue) {
            // The "dummy" field here is required for obscure reasons when generating code on desktop,
            // so we just automatically set it to a dummy value.
            let aru = AvailableRandomizationUnits(clientId: nil, dummy: 0)
            try self.resetTelemetryIdentifiersOnThisThread(aru)
        }
    }
}

extension Nimbus: NimbusStartup {
    public func initialize() {
        catchAll(dbQueue) {
            try self.initializeOnThisThread()
        }
    }

    public func fetchExperiments() {
        catchAll(fetchQueue) {
            try self.fetchExperimentsOnThisThread()
        }
    }

    public func applyPendingExperiments() {
        catchAll(dbQueue) {
            try self.applyPendingExperimentsOnThisThread()
        }
    }

    public func setExperimentsLocally(_ fileURL: URL) {
        catchAll(dbQueue) {
            let json = try String(contentsOf: fileURL)
            try self.setExperimentsLocallyOnThisThread(json)
        }
    }

    public func setExperimentsLocally(_ experimentsJson: String) {
        catchAll(dbQueue) {
            try self.setExperimentsLocallyOnThisThread(experimentsJson)
        }
    }
}

public class NimbusDisabled: NimbusApi {
    public static let shared = NimbusDisabled()

    public var globalUserParticipation: Bool = false
}

public extension NimbusDisabled {
    func getActiveExperiments() -> [EnrolledExperiment] {
        return []
    }

    func getAvailableExperiments() -> [AvailableExperiment] {
        return []
    }

    func getExperimentBranch(featureId _: String) -> String? {
        return nil
    }

    func initialize() {}

    func fetchExperiments() {}

    func applyPendingExperiments() {}

    func setExperimentsLocally(_: URL) {}

    func setExperimentsLocally(_: String) {}

    func optOut(_: String) {}

    func optIn(_: String, branch _: String) {}

    func resetTelemetryIdentifiers() {}

    func getExperimentBranches(_: String) -> [Branch]? {
        return nil
    }
}
