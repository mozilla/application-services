/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import MozillaAppServices

public enum Experiments {
    public static var shared: NimbusApi {
        // Nimbus setup

        // This constructs a `URL` representing the Remote Settings endpoint
        // we can retrieve experiments from.
        var nimbusEndpointUrl: URL {
            var components = URLComponents()
            components.scheme = "https"
            components.host = "firefox.settings.services.mozilla.com"

            // Explicitly unwrapping here since we know this is a valid URL
            return components.url!
        }

        // From the URL we construct the `NimbusServerSettings` struct to
        // inform the Nimbus SDK of the endpoint.
        let nimbusServerSettings = NimbusServerSettings(
            url:  nimbusEndpointUrl
        )

        // The NimbusAppSettings represent the application parameters that
        // are used by the Nimbus SDK for targeting this application with
        // an experiment. The `appName` must be unique to an application,
        // but `channel` can be the same across many apps (typically seen
        // for channel are values like 'nightly', 'beta', and 'release').
        let nimbusAppSettings = NimbusAppSettings(
            appName: "nimbus_ios_sample",
            channel: "sample"
        )

        // This uses a path in the application support directory so that
        // the db doesn't get backed up to iCloud and transferred to
        // another device inadvertently.
        let dbPath = getDirectoryPath(
            named: "nimbus_data"
        )

        // Create a generic NimbusErrorReporter that just prints the
        // error to the console during debugging. The error could also
        // be recorded in other services like Sentry or Glean.
        let nimbusErrorReporter: NimbusErrorReporter = { err in
            print("Nimbus error: \(err)")
        }

        // Create the NimbusApi object and assign it to the local `nimbus`
        // variable. This could potentially throw, so any errors are
        // caught and an error message logged.
        do {
            return try Nimbus.create(
                nimbusServerSettings,
                appSettings: nimbusAppSettings,
                dbPath: dbPath,
                errorReporter: nimbusErrorReporter
            )
        } catch {
            print("Something went wrong durning NimbusApi creation")

            // Nimbus provides a no-op implementation which can be
            // substituted to effectively disable Nimbus.
            return NimbusDisabled.shared
        }
    }

    /// Gets a path in the application support directory
    ///
    /// - parameters:
    ///     * named: the directory name to append to the application support directory path
    ///
    /// - returns: A `String` path
    private static func getDirectoryPath(named: String) -> String {
        let paths = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)
        let documentsDirectory = paths[0]
        return documentsDirectory.appendingPathComponent(named).relativePath
    }
}
