/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation
import MozillaRustComponents

public class Nimbus: NimbusApi {
    private let nimbusClient: NimbusClientProtocol

    private let resourceBundles: [Bundle]

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

    internal init(nimbusClient: NimbusClientProtocol,
                  resourceBundles: [Bundle],
                  errorReporter: @escaping NimbusErrorReporter)
    {
        self.errorReporter = errorReporter
        self.nimbusClient = nimbusClient
        self.resourceBundles = resourceBundles
        NilVariables.instance.set(bundles: resourceBundles)
    }
}

private extension Nimbus {
    func catchAll<T>(_ thunk: () throws -> T?) -> T? {
        do {
            return try thunk()
        } catch NimbusError.DatabaseNotReady {
            return nil
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

extension Nimbus: FeaturesInterface {
    public func recordExposureEvent(featureId: String) {
        catchAll { try nimbusClient.recordExposureEvent(featureId: featureId) }
    }

    internal func postEnrollmentCalculation() {
        // Inform any listeners that we're done here.
        let experiments = getActiveExperiments()
        notifyOnExperimentsApplied(experiments)
    }

    internal func getFeatureConfigVariablesJson(featureId: String) -> [String: Any]? {
        do {
            if let string = try nimbusClient.getFeatureConfigVariables(featureId: featureId),
               let data = string.data(using: .utf8)
            {
                return try JSONSerialization.jsonObject(with: data, options: []) as? [String: Any]
            } else {
                return nil
            }
        } catch {
            errorReporter(error)
            return nil
        }
    }

    public func getVariables(featureId: String, sendExposureEvent: Bool) -> Variables {
        guard let json = getFeatureConfigVariablesJson(featureId: featureId) else {
            return NilVariables.instance
        }

        if sendExposureEvent {
            recordExposureEvent(featureId: featureId)
        }

        return JSONVariables(with: json, in: resourceBundles)
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
        _ = try nimbusClient.setGlobalUserParticipation(optIn: value)
        postEnrollmentCalculation()
    }

    func initializeOnThisThread() throws {
        try nimbusClient.initialize()
    }

    func fetchExperimentsOnThisThread() throws {
        try nimbusClient.fetchExperiments()
        notifyOnExperimentsFetched()
    }

    func applyPendingExperimentsOnThisThread() throws {
        _ = try nimbusClient.applyPendingExperiments()
        postEnrollmentCalculation()
    }

    func setExperimentsLocallyOnThisThread(_ experimentsJson: String) throws {
        try nimbusClient.setExperimentsLocally(experimentsJson: experimentsJson)
    }

    func optOutOnThisThread(_ experimentId: String) throws {
        _ = try nimbusClient.optOut(experimentSlug: experimentId)
        postEnrollmentCalculation()
    }

    func optInOnThisThread(_ experimentId: String, branch: String) throws {
        _ = try nimbusClient.optInWithBranch(experimentSlug: experimentId, branch: branch)
        postEnrollmentCalculation()
    }

    func resetTelemetryIdentifiersOnThisThread(_ identifiers: AvailableRandomizationUnits) throws {
        _ = try nimbusClient.resetTelemetryIdentifiers(newRandomizationUnits: identifiers)
        postEnrollmentCalculation()
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

extension Nimbus: NimbusBranchInterface {
    public func getExperimentBranch(experimentId: String) -> String? {
        return catchAll {
            try nimbusClient.getExperimentBranch(id: experimentId)
        }
    }
}

extension Nimbus: GleanPlumbProtocol {
    public func createMessageHelper() throws -> GleanPlumbMessageHelper {
        return try createMessageHelper(string: nil)
    }

    public func createMessageHelper(additionalContext: [String: Any]) throws -> GleanPlumbMessageHelper {
        let data = try JSONSerialization.data(withJSONObject: additionalContext, options: [])
        let string = String(data: data, encoding: .utf8)
        return try createMessageHelper(string: string)
    }

    public func createMessageHelper<T: Encodable>(additionalContext: T) throws -> GleanPlumbMessageHelper {
        let encoder = JSONEncoder()
        encoder.keyEncodingStrategy = .convertToSnakeCase

        let data = try encoder.encode(additionalContext)
        let string = String(data: data, encoding: .utf8)!
        return try createMessageHelper(string: string)
    }

    private func createMessageHelper(string: String?) throws -> GleanPlumbMessageHelper {
        let targetingHelper = try nimbusClient.createTargetingHelper(additionalContext: string)
        let stringHelper = try nimbusClient.createStringHelper(additionalContext: string)
        return GleanPlumbMessageHelper(targetingHelper: targetingHelper, stringHelper: stringHelper)
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

    func getExperimentBranch(experimentId _: String) -> String? {
        return nil
    }

    func getVariables(featureId _: String, sendExposureEvent _: Bool) -> Variables {
        return NilVariables.instance
    }

    func initialize() {}

    func fetchExperiments() {}

    func applyPendingExperiments() {}

    func setExperimentsLocally(_: URL) {}

    func setExperimentsLocally(_: String) {}

    func optOut(_: String) {}

    func optIn(_: String, branch _: String) {}

    func resetTelemetryIdentifiers() {}

    func recordExposureEvent(featureId _: String) {}

    func getExperimentBranches(_: String) -> [Branch]? {
        return nil
    }
}

extension NimbusDisabled: GleanPlumbProtocol {
    public func createMessageHelper() throws -> GleanPlumbMessageHelper {
        GleanPlumbMessageHelper(
            targetingHelper: AlwaysFalseTargetingHelper(),
            stringHelper: NonStringHelper()
        )
    }

    public func createMessageHelper(additionalContext _: [String: Any]) throws -> GleanPlumbMessageHelper {
        try createMessageHelper()
    }

    public func createMessageHelper<T: Encodable>(additionalContext _: T) throws -> GleanPlumbMessageHelper {
        try createMessageHelper()
    }
}
