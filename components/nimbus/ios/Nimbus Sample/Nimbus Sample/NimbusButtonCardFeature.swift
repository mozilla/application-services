/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import SwiftUI

struct NimbusButtonCardFeature: View {
    @State private var showingAlert = false

    var body: some View {
        nimbusButton
    }
}

private extension NimbusButtonCardFeature {
    var nimbusButton: some View {
        let nimbus = Experiments.shared

        let buttonColorName = nimbus.getExperimentBranch(featureId: "nimbus-button-card-feature")
            ?? "red"

        let color = Color(buttonColorName)

        return Button(buttonColorName, action: { self.showingAlert = true })
            .foregroundColor(color)
            .padding()
            .alert(isPresented: $showingAlert) {
                Alert(
                    title: Text("Nimbus"),
                    message: Text("You pressed the \(color.description) button!"),
                    dismissButton: .default(Text("OK"))
                )
            }
    }

    // TODO: The Feature API work will allow us to have multiple buttons defined in this feature,
    // but right now this only has a single button to experiment on since we can only retrieve
    // the branch name and only be enrolled in a single branch.
}

struct NimbusButtonFeature_Previews: PreviewProvider {
    static var previews: some View {
        NimbusButtonCardFeature()
    }
}
