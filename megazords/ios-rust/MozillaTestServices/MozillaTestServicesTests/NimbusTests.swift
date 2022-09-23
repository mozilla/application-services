/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

@testable import MozillaTestServices

import UIKit
import XCTest

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
                            "enabled": false,
                            "value": {
                                "text": "OK then",
                                "number": 42
                            }
                        }
                    },
                    {
                        "slug": "treatment",
                        "ratio": 1,
                        "feature": {
                            "featureId": "aboutwelcome",
                            "enabled": true,
                            "value": {
                                "text": "OK then",
                                "number": 42
                            }
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
        // For whatever reason, we cannot send a file:// because it'll fail
        // to make the DB both locally and on CI, so we just send the path
        let directory = NSTemporaryDirectory()
        let filename = "testdb-\(UUID().uuidString).db"
        let dbPath = directory + filename
        return dbPath
    }

    func testNimbusCreate() throws {
        let appSettings = NimbusAppSettings(appName: "test", channel: "nightly")
        let nimbusEnabled = try Nimbus.create(nil, appSettings: appSettings, dbPath: createDatabasePath())
        XCTAssert(nimbusEnabled is Nimbus)

        let nimbusDisabled = try Nimbus.create(nil, appSettings: appSettings, dbPath: createDatabasePath(), enabled: false)
        XCTAssert(nimbusDisabled is NimbusDisabled, "Nimbus is disabled if a feature flag disables it")
    }

    func testSmokeTest() throws {
        let appSettings = NimbusAppSettings(appName: "test", channel: "nightly")
        let nimbus = try Nimbus.create(nil, appSettings: appSettings, dbPath: createDatabasePath()) as! Nimbus

        try nimbus.setExperimentsLocallyOnThisThread(minimalExperimentJSON())
        try nimbus.applyPendingExperimentsOnThisThread()

        let branch = nimbus.getExperimentBranch(experimentId: "secure-gold")
        XCTAssertNotNil(branch)
        XCTAssert(branch == "treatment" || branch == "control")

        let experiments = nimbus.getActiveExperiments()
        XCTAssertEqual(experiments.count, 1)

        let json = nimbus.getFeatureConfigVariablesJson(featureId: "aboutwelcome")
        if let json = json {
            XCTAssertEqual(json["text"] as? String, "OK then")
            XCTAssertEqual(json["number"] as? Int, 42)
        } else {
            XCTAssertNotNil(json)
        }

        try nimbus.setExperimentsLocallyOnThisThread(emptyExperimentJSON())
        try nimbus.applyPendingExperimentsOnThisThread()
        let noExperiments = nimbus.getActiveExperiments()
        XCTAssertEqual(noExperiments.count, 0)
    }

    func testSmokeTestAsync() throws {
        let appSettings = NimbusAppSettings(appName: "test", channel: "nightly")
        let nimbus = try Nimbus.create(nil, appSettings: appSettings, dbPath: createDatabasePath()) as! Nimbus

        // We do the same tests as `testSmokeTest` but with the actual calls that
        // the client app will make.
        // This shows that delegating to a background thread is working, and
        // that Rust is callable from a background thread.
        nimbus.setExperimentsLocally(minimalExperimentJSON())
        nimbus.applyPendingExperiments()
        Thread.sleep(until: Date(timeIntervalSinceNow: 1.0))

        let branch = nimbus.getExperimentBranch(experimentId: "secure-gold")
        XCTAssertNotNil(branch)
        XCTAssert(branch == "treatment" || branch == "control")

        let experiments = nimbus.getActiveExperiments()
        XCTAssertEqual(experiments.count, 1)

        nimbus.setExperimentsLocally(emptyExperimentJSON())
        nimbus.applyPendingExperiments()
        Thread.sleep(until: Date(timeIntervalSinceNow: 1.0))

        let noExperiments = nimbus.getActiveExperiments()
        XCTAssertEqual(noExperiments.count, 0)
    }

    func testBuildExperimentContext() throws {
        let appSettings = NimbusAppSettings(appName: "test", channel: "nightly")
        let appContext: AppContext = Nimbus.buildExperimentContext(appSettings)
        NSLog("appContext \(appContext)")
        XCTAssertEqual(appContext.appId, "org.mozilla.MozillaTestServices")
        XCTAssertEqual(appContext.deviceManufacturer, "Apple")
        XCTAssertEqual(appContext.os, "iOS")

        if Device.isSimulator() {
            XCTAssertEqual(appContext.deviceModel, "x86_64")
        }
    }
}

private extension Device {
    static func isSimulator() -> Bool {
        return ProcessInfo.processInfo.environment["SIMULATOR_ROOT"] != nil
    }
}
