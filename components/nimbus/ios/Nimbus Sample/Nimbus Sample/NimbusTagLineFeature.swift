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

        let tagLineText = nimbus.getExperimentBranch(featureId: "nimbus-sample-tagline-feature")
            ?? "Lightning fast experimentation!"

        return Text(tagLineText)
            .padding()
    }
}

struct NimbusTagLineFeature_Previews: PreviewProvider {
    static var previews: some View {
        NimbusTagLineFeature()
    }
}
