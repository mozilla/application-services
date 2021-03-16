/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation

public class Nimbus {
    private let nimbusClient: NimbusClientProtocol

    private let errorReporter: NimbusErrorReporter

    private let dbQueue = DispatchQueue(label: "com.mozilla.nimbus.ios-db", qos: .userInitiated)
    private let fetchQueue = DispatchQueue(label: "com.mozilla.nimbus.ios-network", qos: .background)

    internal init(nimbusClient: NimbusClientProtocol,
                  errorReporter: @escaping NimbusErrorReporter) {
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

    func catchAll(_ queue: DispatchQueue, thunk: @escaping () throws -> Void) {
        queue.async {
            self.catchAll(thunk)
        }
    }
}

// Glean integration
private extension Nimbus {
    func recordExposure(experimentId _: String) {
        // TODO: https://jira.mozilla.com/browse/SDK-209
    }

    func postEnrolmentCalculation(_ events: [EnrollmentChangeEvent]?) {
        guard events?.isEmpty == false else {
            return
        }

        // TODO: https://jira.mozilla.com/browse/SDK-209
        let experiments = getActiveExperiments()
        experiments.forEach { experiment in
            Glean.shared.setExperimentActive(experimentId: experiment.slug, branch: experiment.branchSlug, extra: nil)
        }
        notifyOnExperimentsApplied(experiments)
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
        postEnrolmentCalculation(changes)
    }

    func initializeOnThisThread() throws {
        try nimbusClient.initialize()
    }

    func fetchExperimentsOnThisThread() throws {
        try nimbusClient.fetchExperiments()
    }

    func applyPendingExperimentsOnThisThread() throws {
        let changes = try nimbusClient.applyPendingExperiments()
        postEnrolmentCalculation(changes)
    }

    func setExperimentsLocallyOnThisThread(_ experimentsJson: String) throws {
        try nimbusClient.setExperimentsLocally(experimentsJson: experimentsJson)
    }

    func optOutOnThisThread(_ experimentId: String) throws {
        let changes = try nimbusClient.optOut(experimentSlug: experimentId)
        postEnrolmentCalculation(changes)
    }

    func optInOnThisThread(_ experimentId: String, branch: String) throws {
        let changes = try nimbusClient.optInWithBranch(experimentSlug: experimentId, branch: branch)
        postEnrolmentCalculation(changes)
    }

    func resetTelemetryIdentifiersOnThisThread(_ identifiers: AvailableRandomizationUnits) throws {
        let changes = try nimbusClient.resetTelemetryIdentifiers(newRandomizationUnits: identifiers)
        postEnrolmentCalculation(changes)
    }
}

extension Nimbus: NimbusApi {
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

    public func getExperimentBranch(featureId: String) -> String? {
        return catchAll {
            try nimbusClient.getExperimentBranch(id: featureId)
        }
    }

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

    public func resetTelemetryIdentifiers(_ identifiers: AvailableRandomizationUnits) {
        catchAll(dbQueue) {
            try self.resetTelemetryIdentifiersOnThisThread(identifiers)
        }
    }
}

public class NimbusDisabled: NimbusApi {
    public var globalUserParticipation: Bool = false
}

public extension NimbusDisabled {
    func getActiveExperiments() -> [EnrolledExperiment] {
        return []
    }

    func getExperimentBranch(featureId _: String) -> String? {
        return nil
    }

    func initialize() {
        return
    }

    func fetchExperiments() {
        return
    }

    func applyPendingExperiments() {
        return
    }

    func setExperimentsLocally(_: URL) {
        return
    }

    func setExperimentsLocally(_: String) {
        return
    }

    func optOut(_: String) {
        return
    }

    func optIn(_: String, branch _: String) {
        return
    }

    func resetTelemetryIdentifiers(_: AvailableRandomizationUnits) {
        return
    }
}
