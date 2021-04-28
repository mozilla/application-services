/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import SwiftUI

@main
struct Nimbus_SampleApp: App {
    init() {
        // Retrieve the singleton instance
        let nimbus = Experiments.shared

        // Initialize the Nimbus SDK
        nimbus.initialize()

        //TODO: Set experiments locally

        // Apply any experiment updates, either set locally or fetched
        // during the previous run of the application
        nimbus.applyPendingExperiments()

        // Fetch new experiments in the background from the Remote Settings
        // endpoint, to be applied on the next invocation of the app.
        nimbus.fetchExperiments()
    }

    var body: some Scene {
        WindowGroup {
            ContentView()
        }
    }
}
