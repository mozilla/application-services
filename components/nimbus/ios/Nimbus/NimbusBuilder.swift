/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation

/**
 * A builder for [Nimbus] singleton objects, parameterized in a declarative class.
 */
public class NimbusBuilder {
    let dbFilePath: String

    public init(dbPath: String) {
        dbFilePath = dbPath
    }

    /**
     * An optional server URL string.
     *
     * This will only be null or empty in development or testing, or in any build variant of a
     * non-Mozilla fork.
     */
    @discardableResult
    public func with(url: String?) -> Self {
        self.url = url
        return self
    }

    var url: String?

    /**
     * A closure for reporting errors from Rust.
     */
    @discardableResult
    public func with(errorReporter reporter: @escaping NimbusErrorReporter) -> NimbusBuilder {
        errorReporter = reporter
        return self
    }

    var errorReporter: NimbusErrorReporter = defaultErrorReporter

    /**
     * A flag to select the main or preview collection of remote settings. Defaults to `false`.
     */
    @discardableResult
    public func using(previewCollection flag: Bool) -> NimbusBuilder {
        usePreviewCollection = flag
        return self
    }

    var usePreviewCollection: Bool = false

    /**
     * A flag to indicate if this is being run on the first run of the app. This is used to control
     * whether the `initial_experiments` file is used to populate Nimbus.
     */
    @discardableResult
    public func isFirstRun(_ flag: Bool) -> NimbusBuilder {
        isFirstRun = flag
        return self
    }

    var isFirstRun: Bool = true

    /**
     * A optional raw resource of a file downloaded at or near build time from Remote Settings.
     */
    @discardableResult
    public func with(initialExperiments fileURL: URL?) -> NimbusBuilder {
        initialExperiments = fileURL
        return self
    }

    var initialExperiments: URL?

    /**
     * The timeout used to wait for the loading of the `initial_experiments
     */
    @discardableResult
    public func with(timeoutForLoadingInitialExperiments seconds: TimeInterval) -> NimbusBuilder {
        timeoutLoadingExperiment = seconds
        return self
    }

    var timeoutLoadingExperiment: TimeInterval = 0.200 /* seconds */

    /**
     * Optional callback to be called after the creation of the nimbus object and it is ready
     * to be used.
     */
    @discardableResult
    public func onCreate(callback: @escaping (NimbusInterface) -> Void) -> NimbusBuilder {
        onCreateCallback = callback
        return self
    }

    var onCreateCallback: ((NimbusInterface) -> Void)?

    /**
     * Optional callback to be called after the calculatoin of new enrollments and applying of changes to
     * experiments recipes.
     */
    @discardableResult
    public func onApply(callback: @escaping (NimbusInterface) -> Void) -> NimbusBuilder {
        onApplyCallback = callback
        return self
    }

    var onApplyCallback: ((NimbusInterface) -> Void)?

    /**
     * Resource bundles used to look up bundled text and images. Defaults to `[Bundle.main]`.
     */
    @discardableResult
    public func with(bundles: [Bundle]) -> NimbusBuilder {
        resourceBundles = bundles
        return self
    }

    var resourceBundles: [Bundle] = [.main]

    /**
     * The object generated from the `nimbus.fml.yaml` file and the nimbus-gradle-plugin.
     */
    @discardableResult
    public func with(featureManifest: FeatureManifestInterface) -> NimbusBuilder {
        self.featureManifest = featureManifest
        return self
    }

    var featureManifest: FeatureManifestInterface?

    /**
     * The command line arguments for the app. This is useful for QA, and can be safely left in the app in production.
     */
    @discardableResult
    public func with(commandLineArgs: [String]) -> NimbusBuilder {
        self.commandLineArgs = commandLineArgs
        return self
    }

    var commandLineArgs: [String]?

    /**
     * Build a [Nimbus] singleton for the given [NimbusAppSettings]. Instances built with this method
     * have been initialized, and are ready for use by the app.
     *
     * Instance have _not_ yet had [fetchExperiments()] called on it, or anything usage of the
     * network. This is to allow the networking stack to be initialized after this method is called
     * and the networking stack to be involved in experiments.
     */
    public func build(appInfo: NimbusAppSettings) -> NimbusInterface {
        let serverSettings: NimbusServerSettings?
        if let string = url,
           let url = URL(string: string)
        {
            if usePreviewCollection {
                serverSettings = NimbusServerSettings(url: url, collection: remoteSettingsPreviewCollection)
            } else {
                serverSettings = NimbusServerSettings(url: url, collection: remoteSettingsCollection)
            }
        } else {
            serverSettings = nil
        }

        do {
            let nimbus = try newNimbus(appInfo, serverSettings: serverSettings)
            let fm = featureManifest
            let onApplyCallback = onApplyCallback
            if fm != nil || onApplyCallback != nil {
                NotificationCenter.default.addObserver(forName: .nimbusExperimentsApplied,
                                                       object: nil,
                                                       queue: nil)
                { _ in
                    fm?.invalidateCachedValues()
                    onApplyCallback?(nimbus)
                }
            }
            if let args = unpack(args: commandLineArgs), let experiments = args["experiments"] {
                // If we have command line arguments, then load experiments from there,
                // and disable future fetching.
                nimbus.setExperimentsLocally(experiments)
                nimbus.applyPendingExperiments().waitUntilFinished()
                // setExperimentsLocally and applyPendingExperiments run on the
                // same single threaded dispatch queue, so we can run them in series,
                // and wait for the apply.
                nimbus.setFetchEnabled(false)
            } else if let file = initialExperiments, isFirstRun || serverSettings == nil {
                let job = nimbus.applyLocalExperiments(fileURL: file)
                _ = job.joinOrTimeout(timeout: timeoutLoadingExperiment)
            } else {
                nimbus.applyPendingExperiments().waitUntilFinished()
            }

            // By now, on this thread, we have a fully initialized Nimbus object, ready for use:
            // * we gave a 200ms timeout to the loading of a file from res/raw
            // * on completion or cancellation, applyPendingExperiments or initialize was
            //   called, and this thread waited for that to complete.
            featureManifest?.initialize { nimbus }
            onCreateCallback?(nimbus)

            return nimbus
        } catch {
            errorReporter(error)
            return newNimbusDisabled()
        }
    }

    private func unpack(args: [String]?) -> [String: String]? {
        guard let args = args else {
            return nil
        }
        if !args.contains("--nimbus-cli") {
            return nil
        }

        var argMap = [String: String]()
        var key: String?
        args.forEach { arg in
            var value: String?
            switch arg {
            case "--version":
                key = "version"
            case "--experiments":
                key = "experiments"
            default:
                value = arg
            }

            if let k = key, let v = value {
                argMap[k] = v
                key = nil
                value = nil
            }
        }

        if argMap["version"] != "1" {
            return nil
        }

        guard let experiments = argMap["experiments"],
              let payload = try? Dictionary.parse(jsonString: experiments),
              payload["data"] is [Any]
        else {
            return nil
        }

        return argMap
    }

    func newNimbus(_ appInfo: NimbusAppSettings, serverSettings: NimbusServerSettings?) throws -> NimbusInterface {
        try Nimbus.create(serverSettings, appSettings: appInfo, dbPath: dbFilePath,
                          resourceBundles: resourceBundles, errorReporter: errorReporter)
    }

    func newNimbusDisabled() -> NimbusInterface {
        NimbusDisabled.shared
    }
}
