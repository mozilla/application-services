/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation

private let LOG_TAG = "service/Nimbus"
private let EXPERIMENT_BUCKET_NAME = "main"
private let EXPERIMENT_COLLECTION_NAME = "nimbus-mobile-experiments"

private let logger = Logger(tag: LOG_TAG)

public extension Notification.Name {
    static let nimbusExperimentsFetched = Notification.Name("nimbusExperimentsFetched")
    static let nimbusExperimentsApplied = Notification.Name("nimbusExperimentsApplied")
}

public typealias NimbusErrorReporter = (Error) -> ()

public let defaultErrorReporter: NimbusErrorReporter = { err in
    switch err {
    case is LocalizedError:
        let description = err.localizedDescription
        logger.error("Nimbus error: \(description)")
    default:
        logger.error("Nimbus error: \(err)")
    }
}

public struct NimbusServerSettings {
    let url: URL
}

public struct NimbusAppSettings {
    let appName: String
    let channel: String
}

public class Nimbus: NimbusApi {
    private let nimbusClient: NimbusClientProtocol

    private let errorReporter: NimbusErrorReporter

    private let dbQueue = DispatchQueue(label: "com.mozilla.nimbus.ios-db", qos: .userInitiated)
    private let fetchQueue = DispatchQueue(label: "com.mozilla.nimbus.ios-network", qos: .background)

    public static func create(_ server: NimbusServerSettings?,
                              appSettings: NimbusAppSettings,
                              dbPath: String,
                              enabled: Bool = true,
                              errorReporter: @escaping NimbusErrorReporter = defaultErrorReporter
    ) -> NimbusApi {

        guard enabled else {
            return NimbusDisabled()
        }

        let context = Nimbus.buildExperimentContext(appSettings)
        let remoteSettings = server.map { server -> RemoteSettingsConfig in
            let url = server.url.absoluteString
            return RemoteSettingsConfig(
                serverUrl: url,
                bucketName: EXPERIMENT_BUCKET_NAME,
                collectionName: EXPERIMENT_COLLECTION_NAME
            )
        }

        do {
            let nimbusClient = try NimbusClient(
                appCtx: context,
                dbpath: dbPath,
                remoteSettingsConfig: remoteSettings,
                // The "dummy" field here is required for obscure reasons when generating code on desktop,
                // so we just automatically set it to a dummy value.
                availableRandomizationUnits: AvailableRandomizationUnits(clientId: nil, dummy: 0)
            )

            return Nimbus(nimbusClient: nimbusClient, errorReporter: errorReporter)
        } catch {
            errorReporter(error)
            return NimbusDisabled()
        }
    }

    public static func buildExperimentContext(_ appSettings: NimbusAppSettings,
                                              bundle: Bundle = Bundle.main,
                                              device: UIDevice = .current
    ) -> AppContext {
        let info = bundle.infoDictionary ?? [:]
        return AppContext(appId: info["CFBundleIdentifier"] as? String ?? "unknown",
                          appVersion: info["CFBundleShortVersionString"] as? String,
                          appBuild: info["CFBundleVersion"] as? String,
                          architecture: nil,
                          deviceManufacturer: Sysctl.manufacturer,
                          deviceModel: Sysctl.model,
                          locale: Locale.current.identifier,
                          os: device.systemName,
                          osVersion: device.systemVersion,
                          androidSdkVersion: nil,
                          debugTag: LOG_TAG)
    }

    init(nimbusClient: NimbusClientProtocol,
         errorReporter: @escaping NimbusErrorReporter = defaultErrorReporter
    ) {
        self.errorReporter = errorReporter
        self.nimbusClient = nimbusClient
    }
}

private extension Nimbus {
    func catchAll<T>(_ thunk: () throws -> T?) -> T? {
        do {
            return try thunk()
        } catch {
            self.errorReporter(error)
            return nil
        }
    }

    func catchAll(_ queue: DispatchQueue, thunk: @escaping () throws -> ()) {
        queue.async {
            self.catchAll(thunk)
        }
    }
}


// Glean integration
private extension Nimbus {
    func recordExposure(experimentId: String) {
        // TODO https://jira.mozilla.com/browse/SDK-209
    }

    func postEnrolmentCalculation(_ events: [EnrollmentChangeEvent]?) {
        guard events?.isEmpty == false else {
            return
        }

        // TODO https://jira.mozilla.com/browse/SDK-209
        let experiments = self.getActiveExperiments()
        experiments.forEach { experiment in
            Glean.shared.setExperimentActive(experimentId: experiment.slug, branch: experiment.branchSlug, extra: nil)
        }
    }
}

/*
 * Methods split out onto a separate internal extension for testing purposes.
 */
internal extension Nimbus {
    func setGlobalUserParticipationOnThisThread(_ value: Bool) throws {
        let changes = try self.nimbusClient.setGlobalUserParticipation(optIn: value)
        self.postEnrolmentCalculation(changes)
    }

    func initializeOnThisThread() throws {
        try self.nimbusClient.initialize()
    }

    func fetchExperimentsOnThisThread() throws {
        try self.nimbusClient.fetchExperiments()
    }

    func applyPendingExperimentsOnThisThread() throws {
        let changes = try self.nimbusClient.applyPendingExperiments()
        self.postEnrolmentCalculation(changes)
    }

    func setExperimentsLocallyOnThisThread(_ experimentsJson: String) throws {
        try self.nimbusClient.setExperimentsLocally(experimentsJson: experimentsJson)
    }

    func optOutOnThisThread(_ experimentId: String) throws {
        let changes = try self.nimbusClient.optOut(experimentSlug: experimentId)
        self.postEnrolmentCalculation(changes)
    }

    func resetTelemetryIdentifiersOnThisThread(_ identifiers: AvailableRandomizationUnits) throws {
        let changes = try self.nimbusClient.resetTelemetryIdentifiers(newRandomizationUnits: identifiers)
        self.postEnrolmentCalculation(changes)
    }
}

public extension Nimbus {
    var globalUserParticipation: Bool {
        get {
            catchAll { try nimbusClient.getGlobalUserParticipation() } ?? false
        }
        set {
            catchAll(dbQueue) {
                try self.setGlobalUserParticipationOnThisThread(newValue)
            }
        }
    }

    func getActiveExperiments() -> [EnrolledExperiment] {
        return catchAll {
            try nimbusClient.getActiveExperiments()
        } ?? []
    }

    func getExperimentBranch(featureId: String) -> String? {
        return catchAll {
            try nimbusClient.getExperimentBranch(id: featureId)
        }
    }

    func initialize() {
        catchAll(dbQueue) {
            try self.initializeOnThisThread()
        }
    }

    func fetchExperiments() {
        catchAll(fetchQueue) {
            try self.fetchExperimentsOnThisThread()
        }
    }

    func applyPendingExperiments() {
        catchAll(dbQueue) {
            try self.applyPendingExperimentsOnThisThread()
        }
    }

    func setExperimentsLocally(_ experimentsJson: String) {
        catchAll(dbQueue) {
            try self.setExperimentsLocallyOnThisThread(experimentsJson)
        }
    }

    func optOut(_ experimentId: String) {
        catchAll(dbQueue) {
            try self.optOutOnThisThread(experimentId)
        }
    }

    func resetTelemetryIdentifiers(_ identifiers: AvailableRandomizationUnits) {
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
    func getExperimentBranch(featureId: String) -> String? {
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
    func setExperimentsLocally(_ experimentsJson: String) {
        return
    }
    func optOut(_ experimentId: String) {
        return
    }
    func resetTelemetryIdentifiers(_ identifiers: AvailableRandomizationUnits) {
        return
    }
}
