/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation

private let remoteSettingsBucket = "main"
private let remoteSettingsCollection = "nimbus-mobile-experiments"

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

public extension Nimbus {
    /// Create an instance of `Nimbus`.
    ///
    /// - Parameters:
    ///     - server: the server that experiments will be downloaded from
    ///     - appSettings: the name and channel for the app
    ///     - dbPath: the path on disk for the database
    ///     - enabled: intended for FeatureFlags. If false, then return a dummy `Nimbus` instance. Defaults to `true`.
    ///     - errorReporter: a closure capable of reporting errors. Defaults to using a logger.
    /// - Returns an implementation of `NimbusApi`.
    /// - Throws `NimbusError` if anything goes wrong with the Rust FFI or in the `NimbusClient` constructor.
    ///
    static func create(
        _ server: NimbusServerSettings?,
        appSettings: NimbusAppSettings,
        dbPath: String,
        enabled: Bool = true,
        errorReporter: @escaping NimbusErrorReporter = defaultErrorReporter
    ) throws -> NimbusApi {
        guard enabled else {
            return NimbusDisabled.shared
        }

        let context = Nimbus.buildExperimentContext(appSettings)
        let remoteSettings = server.map { server -> RemoteSettingsConfig in
            let url = server.url.absoluteString
            return RemoteSettingsConfig(
                serverUrl: url,
                bucketName: remoteSettingsBucket,
                collectionName: remoteSettingsCollection
            )
        }
        let nimbusClient = try NimbusClient(
            appCtx: context,
            dbpath: dbPath,
            remoteSettingsConfig: remoteSettings,
            // The "dummy" field here is required for obscure reasons when generating code on desktop,
            // so we just automatically set it to a dummy value.
            availableRandomizationUnits: AvailableRandomizationUnits(clientId: nil, dummy: 0)
        )

        return Nimbus(nimbusClient: nimbusClient, errorReporter: errorReporter)
    }

    static func buildExperimentContext(
        _ appSettings: NimbusAppSettings,
        bundle: Bundle = Bundle.main,
        device: UIDevice = .current
    ) -> AppContext {
        let info = bundle.infoDictionary ?? [:]
        return AppContext(
            appName: appSettings.appName,
            appId: info["CFBundleIdentifier"] as? String ?? "unknown",
            channel: appSettings.channel,
            appVersion: info["CFBundleShortVersionString"] as? String,
            appBuild: info["CFBundleVersion"] as? String,
            architecture: Sysctl.machine, // Sysctl is from Glean.
            deviceManufacturer: Sysctl.manufacturer,
            deviceModel: Sysctl.model,
            locale: getLocaleTag(), // from Glean utils
            os: device.systemName,
            osVersion: device.systemVersion,
            androidSdkVersion: nil,
            debugTag: "Nimbus.rs"
        )
    }
}
