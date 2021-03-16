/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation

private let EXPERIMENT_BUCKET_NAME = "main"
private let EXPERIMENT_COLLECTION_NAME = "nimbus-mobile-experiments"

private let LOG_TAG = "Nimbus.swift"
private let logger = Logger(tag: LOG_TAG)

public let defaultErrorReporter: NimbusErrorReporter = { err in
    switch err {
    case is LocalizedError:
        let description = err.localizedDescription
        logger.error("Nimbus error: \(description)")
    default:
        logger.error("Nimbus error: \(err)")
    }
}

extension Nimbus {
    /**
     *  Create an instance of `Nimbus`.
     *
     * - Parameters:
     *     - server: the server that experiments will be downloaded from
     *     - appSettings: the name and channel for the app
     *     - dbPath: the path on disk for the database
     *     - enabled: intended for FeatureFlags. If false, then return a dummy `Nimbus` instance. Defaults to `true`.
     *     - errorReporter: a closure capable of reporting errors. Defaults to using a logger.
     */
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
                          debugTag: "Nimbus.rs")
    }
}
