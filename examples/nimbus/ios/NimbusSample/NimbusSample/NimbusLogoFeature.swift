/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import SwiftUI

struct NimbusLogoFeature: View {
    var body: some View {
        nimbusLogo
    }
}

private extension NimbusLogoFeature {
    var nimbusLogo: some View {
        let nimbus = Experiments.shared

        // First we retrieve the variables for this feature. These are supplied by Nimbus
        // from a cache and may change after calling `applyPendingExperiments`.
        let featureVariables = nimbus.getVariables(featureId: "nimbus-sample-logo-feature")

        // Retrieve the experimental value for the logo image
        let logoImage = featureVariables.getString("nimbus-logo")
            ?? "NimbusLogo" // Otherwise, use the default

        return Image(logoImage)
            .resizable()
            .padding()
            .aspectRatio(contentMode: .fit)
    }
}

struct NimbusLogoFeature_Previews: PreviewProvider {
    static var previews: some View {
        NimbusLogoFeature()
    }
}
