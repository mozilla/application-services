/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import SwiftUI

struct NimbusTagLineFeature: View {
    var body: some View {
        nimbusTagLine
    }
}

private extension NimbusTagLineFeature {
    var nimbusTagLine: some View {
        let nimbus = Experiments.shared

        // First we retrieve the variables for this feature. These are supplied by Nimbus
        // from a cache and may change after calling `applyPendingExperiments`.
        let featureVariables = nimbus.getVariables(featureId: "nimbus-sample-tagline-feature")

        // Retrieve the experimental value for the tag line text
        let tagLineText = featureVariables.getText("tag-line-text")
            ?? "Lightning fast experimentation!" // Otherwise, use the default text

        return Text(tagLineText)
            .padding()
    }
}

struct NimbusTagLineFeature_Previews: PreviewProvider {
    static var previews: some View {
        NimbusTagLineFeature()
    }
}
