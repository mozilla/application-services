/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation

public protocol NimbusApi {
    func getActiveExperiments() -> [EnrolledExperiment]
    func getExperimentBranch(featureId: String) -> String?
    func initialize()
    func fetchExperiments()
    func applyPendingExperiments()
    func setExperimentsLocally(_ experimentsJson: String)
    func optOut(_ experimentId: String)
    func resetTelemetryIdentifiers(_ identifiers: AvailableRandomizationUnits)
    var globalUserParticipation: Bool { get set }
}
