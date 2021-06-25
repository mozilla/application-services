/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import SwiftUI
import Viaduct
import Glean

@main
struct NimbusSampleApp: App {
    init() {
        // Enable the viaduct reqwest backend
        Viaduct.shared.useReqwestBackend()

        // Initialize Glean
        setupGlean()

        // Initialize Nimbus
        setupNimbus()
    }

    var body: some Scene {
        WindowGroup {
            ContentView()
        }
    }

    private func setupGlean() {
        let glean = Glean.shared

        glean.initialize(uploadEnabled: true)
    }

    private func setupNimbus() {
        // Retrieve the singleton instance of Nimbus from the Experiments
        // module
        let nimbus = Experiments.shared

        // Initialize the Nimbus SDK. This prepares the SDK and loads the
        // underlying database and caches so that experiment values can be
        // retrieved.
        nimbus.initialize()

        // TODO: Set experiments locally

        // Apply any experiment updates, either set locally or fetched
        // during the previous run of the application
        nimbus.applyPendingExperiments()

        // Fetch new experiments in the background from the Remote Settings
        // endpoint, to be applied on the next invocation of the app.
        nimbus.fetchExperiments()
    }
}
