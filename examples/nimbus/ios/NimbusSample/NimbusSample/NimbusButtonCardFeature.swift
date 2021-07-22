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

        // First we retrieve the variables for this feature. These are supplied by Nimbus
        // from a cache and may change after calling `applyPendingExperiments`.
        let featureVariables = nimbus.getVariables(featureId: "nimbus-button-card-feature")

        // Now we can select variables or a default for the experimentable parameters of
        // this feature.
        let buttonColorName = featureVariables.getString("button-color") ?? "red"

        let color = Color(UIColor(named: buttonColorName) ?? UIColor.red)

        return Button(buttonColorName, action: { self.showingAlert = true })
            .foregroundColor(color)
            .padding()
            .overlay(
                RoundedRectangle(cornerRadius: 15)
                    .stroke(lineWidth: 2.0)
                    .frame(width: 200, height: 50, alignment: .center)
            )
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
