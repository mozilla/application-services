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

            let job: Operation
            if let file = initialExperiments, isFirstRun || serverSettings == nil {
                job = nimbus.applyLocalExperiments(fileURL: file)
            } else {
                job = nimbus.applyPendingExperiments()
            }

            _ = job.joinOrTimeout(timeout: timeoutLoadingExperiment)

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

    func newNimbus(_ appInfo: NimbusAppSettings, serverSettings: NimbusServerSettings?) throws -> NimbusInterface {
        try Nimbus.create(serverSettings, appSettings: appInfo, dbPath: dbFilePath,
                          resourceBundles: resourceBundles, errorReporter: errorReporter)
    }

    func newNimbusDisabled() -> NimbusInterface {
        NimbusDisabled.shared
    }
}
