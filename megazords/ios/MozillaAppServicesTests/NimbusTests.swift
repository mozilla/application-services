/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import XCTest

@testable import MozillaAppServices

class NimbusTests: XCTestCase {

    func emptyExperimentJSON() -> String {
        return """
            { "data": [] }
            """
    }

    func minimalExperimentJSON() -> String {
        return """
            {
                "data": [{
                    "schemaVersion": "1.0.0",
                    "slug": "secure-gold",
                    "endDate": null,
                    "featureIds": ["aboutwelcome"],
                    "branches": [{
                            "slug": "control",
                            "ratio": 1,
                            "feature": {
                                "featureId": "aboutwelcome",
                                "enabled": false
                            }
                        },
                        {
                            "slug": "treatment",
                            "ratio": 1,
                            "feature": {
                                "featureId": "aboutwelcome",
                                "enabled": true
                            }
                        }
                    ],
                    "probeSets": [],
                    "startDate": null,
                    "application": "\(xcTestAppId())",
                    "bucketConfig": {
                        "count": 10000,
                        "start": 0,
                        "total": 10000,
                        "namespace": "secure-gold",
                        "randomizationUnit": "nimbus_id"
                    },
                    "userFacingName": "Diagnostic test experiment",
                    "referenceBranch": "control",
                    "isEnrollmentPaused": false,
                    "proposedEnrollment": 7,
                    "userFacingDescription": "This is a test experiment for diagnostic purposes.",
                    "id": "secure-gold",
                    "last_modified": 1602197324372
                }]
            }
            """
    }

    func xcTestAppId() -> String {
        return "com.apple.dt.xctest.tool"
    }

    func createDatabasePath() -> String {
        let directory = NSTemporaryDirectory()
        let filename = "testdb-\(UUID().uuidString).db"
        let fileURL = URL(fileURLWithPath: directory).appendingPathComponent(filename)
        return fileURL.absoluteString
    }

    func testNimbusCreate() throws {
        let appSettings = NimbusAppSettings(appName: "test", channel: "nightly")
        let nimbusEnabled = Nimbus.create(nil, appSettings: appSettings, dbPath: createDatabasePath())
        XCTAssert(nimbusEnabled is Nimbus)

        let nimbusDisabled = Nimbus.create(nil, appSettings: appSettings, dbPath: createDatabasePath(), enabled: false)
        XCTAssert(nimbusDisabled is NimbusDisabled, "Nimbus is disabled if a feature flag disables it")
    }

    func testSmokeTest() throws {
        let appSettings = NimbusAppSettings(appName: "test", channel: "nightly")
        let nimbus = Nimbus.create(nil, appSettings: appSettings, dbPath: createDatabasePath()) as! Nimbus

        try nimbus.setExperimentsLocallyOnThisThread(minimalExperimentJSON())
        try nimbus.applyPendingExperimentsOnThisThread()

        let branch = nimbus.getExperimentBranch(featureId: "aboutwelcome")
        XCTAssertNotNil(branch)
        XCTAssert(branch == "treatment" || branch == "control")

        let experiments = nimbus.getActiveExperiments()
        XCTAssertEqual(experiments.count, 1)

        try nimbus.setExperimentsLocallyOnThisThread(emptyExperimentJSON())
        try nimbus.applyPendingExperimentsOnThisThread()
        let noExperiments = nimbus.getActiveExperiments()
        XCTAssertEqual(noExperiments.count, 0)
    }

    func testBuildExperimentContext() throws {
        let appSettings = NimbusAppSettings(appName: "test", channel: "nightly")
        let appContext: AppContext = Nimbus.buildExperimentContext(appSettings)
        NSLog("appContext \(appContext)")
        XCTAssertEqual(appContext.appId, xcTestAppId())
        XCTAssertEqual(appContext.deviceManufacturer, "Apple")
        XCTAssertEqual(appContext.os, "iOS")

        if Device.isSimulator() {
            XCTAssertEqual(appContext.deviceModel, "x86_64")
        }
    }
}

extension Device {
    static func isSimulator() -> Bool {
        return ProcessInfo.processInfo.environment["SIMULATOR_ROOT"] != nil
    }
}
