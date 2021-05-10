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

        let imageName = nimbus.getExperimentBranch(featureId: "nimbus-sample-logo-feature") ?? "NimbusLogo"

        return Image(imageName)
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
