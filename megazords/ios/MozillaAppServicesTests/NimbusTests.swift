/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import XCTest

@testable import MozillaAppServices

class NimbusTests: XCTestCase {
    override func setUp() {
        Glean.shared.resetGlean(clearStores: true)
        Glean.shared.enableTestingMode()
        let buildDate = DateComponents(
            calendar: Calendar.current,
            timeZone: TimeZone(abbreviation: "UTC"),
            year: 2019,
            month: 10,
            day: 23,
            hour: 12,
            minute: 52,
            second: 8
        )
        let buildInfo = BuildInfo(buildDate: buildDate)
        Glean.shared.initialize(
            uploadEnabled: true,
            configuration: Configuration(
                channel: "test",
                serverEndpoint: "https://example.com"
            ),
            buildInfo: buildInfo
        )
    }

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
        let directory = NSTemporaryDirectory()
        let filename = "testdb-\(UUID().uuidString).db"
        let fileURL = URL(fileURLWithPath: directory).appendingPathComponent(filename)
        return fileURL.absoluteString
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
        XCTAssertEqual(appContext.appId, xcTestAppId())
        XCTAssertEqual(appContext.deviceManufacturer, "Apple")
        XCTAssertEqual(appContext.os, "iOS")

        if Device.isSimulator() {
            XCTAssertEqual(appContext.deviceModel, "x86_64")
        }
    }

    func testRecordExperimentTelemetry() throws {
        let appSettings = NimbusAppSettings(appName: "NimbusUnitTest", channel: "test")
        let nimbus = try Nimbus.create(nil, appSettings: appSettings, dbPath: createDatabasePath()) as! Nimbus

        let enrolledExperiments = [EnrolledExperiment(
            featureIds: [],
            slug: "test-experiment",
            userFacingName: "Test Experiment",
            userFacingDescription: "A test experiment for testing experiments",
            branchSlug: "test-branch",
            enrollmentId: "enrollment-id"
        )]

        nimbus.recordExperimentTelemetry(enrolledExperiments)
        XCTAssertTrue(Glean.shared.testIsExperimentActive(experimentId: "test-experiment"),
                      "Experiment should be active")
        let experimentData = Glean.shared.testGetExperimentData(experimentId: "test-experiment")!
        XCTAssertEqual("test-branch", experimentData.branch, "Experiment branch must match")
        XCTAssertEqual("enrollment-id", experimentData.extra["enrollmentId"], "Enrollment id must match")
    }

    func testRecordExperimentEvents() throws {
        let appSettings = NimbusAppSettings(appName: "NimbusUnitTest", channel: "test")
        let nimbus = try Nimbus.create(nil, appSettings: appSettings, dbPath: createDatabasePath()) as! Nimbus

        // Create a list of events to record, one of each type, all associated with the same
        // experiment
        let events = [
            EnrollmentChangeEvent(
                experimentSlug: "test-experiment",
                branchSlug: "test-branch",
                enrollmentId: "test-enrollment-id",
                reason: "test-reason",
                change: .enrollment
            ),
            EnrollmentChangeEvent(
                experimentSlug: "test-experiment",
                branchSlug: "test-branch",
                enrollmentId: "test-enrollment-id",
                reason: "test-reason",
                change: .unenrollment
            ),
            EnrollmentChangeEvent(
                experimentSlug: "test-experiment",
                branchSlug: "test-branch",
                enrollmentId: "test-enrollment-id",
                reason: "test-reason",
                change: .disqualification
            ),
        ]

        // Record the experiment events in Glean
        nimbus.recordExperimentEvents(events)

        // Use the Glean test API to check the recorded events

        // Enrollment
        XCTAssertTrue(GleanMetrics.NimbusEvents.enrollment.testHasValue(), "Enrollment event must exist")
        let enrollmentEvents = try GleanMetrics.NimbusEvents.enrollment.testGetValue()
        XCTAssertEqual(1, enrollmentEvents.count, "Enrollment event count must match")
        let enrollmentEventExtras = enrollmentEvents.first!.extra
        XCTAssertEqual("test-experiment", enrollmentEventExtras!["experiment"], "Enrollment event experiment must match")
        XCTAssertEqual("test-branch", enrollmentEventExtras!["branch"], "Enrollment event branch must match")
        XCTAssertEqual("test-enrollment-id", enrollmentEventExtras!["enrollment_id"], "Enrollment event enrollment id must match")

        // Unenrollment
        XCTAssertTrue(GleanMetrics.NimbusEvents.unenrollment.testHasValue(), "Unenrollment event must exist")
        let unenrollmentEvents = try GleanMetrics.NimbusEvents.unenrollment.testGetValue()
        XCTAssertEqual(1, unenrollmentEvents.count, "Unenrollment event count must match")
        let unenrollmentEventExtras = unenrollmentEvents.first!.extra
        XCTAssertEqual("test-experiment", unenrollmentEventExtras!["experiment"], "Unenrollment event experiment must match")
        XCTAssertEqual("test-branch", unenrollmentEventExtras!["branch"], "Unenrollment event branch must match")
        XCTAssertEqual("test-enrollment-id", unenrollmentEventExtras!["enrollment_id"], "Unenrollment event enrollment id must match")

        // Disqualification
        XCTAssertTrue(GleanMetrics.NimbusEvents.disqualification.testHasValue(), "Disqualification event must exist")
        let disqualificationEvents = try GleanMetrics.NimbusEvents.disqualification.testGetValue()
        XCTAssertEqual(1, disqualificationEvents.count, "Disqualification event count must match")
        let disqualificationEventExtras = disqualificationEvents.first!.extra
        XCTAssertEqual("test-experiment", disqualificationEventExtras!["experiment"], "Disqualification event experiment must match")
        XCTAssertEqual("test-branch", disqualificationEventExtras!["branch"], "Disqualification event branch must match")
        XCTAssertEqual("test-enrollment-id", disqualificationEventExtras!["enrollment_id"], "Disqualification event enrollment id must match")
    }

    func testRecordExposure() throws {
        let appSettings = NimbusAppSettings(appName: "NimbusUnitTest", channel: "test")
        let nimbus = try Nimbus.create(nil, appSettings: appSettings, dbPath: createDatabasePath()) as! Nimbus

        // Load an experiment in nimbus that we will record an event in. The experiment bucket configuration
        // is set so that it will be guaranteed to be active. This is necessary because the SDK checks for
        // active experiments before recording.
        try nimbus.setExperimentsLocallyOnThisThread(minimalExperimentJSON())
        try nimbus.applyPendingExperimentsOnThisThread()

        // Assert that there are no events to start with
        XCTAssertFalse(GleanMetrics.NimbusEvents.exposure.testHasValue(), "Event must have a value")

        // Record a valid exposure event in Glean that matches the featureId from the test experiment
        nimbus.recordExposureEvent(featureId: "aboutwelcome")

        // Use the Glean test API to check that the valid event is present
        XCTAssertTrue(GleanMetrics.NimbusEvents.exposure.testHasValue(), "Event must have a value")
        let enrollmentEvents = try GleanMetrics.NimbusEvents.exposure.testGetValue()
        XCTAssertEqual(1, enrollmentEvents.count, "Event count must match")
        let enrollmentEventExtras = enrollmentEvents.first!.extra
        XCTAssertEqual("secure-gold", enrollmentEventExtras!["experiment"], "Experiment slug must match")
        XCTAssertTrue(
            enrollmentEventExtras!["branch"] == "control" || enrollmentEventExtras!["branch"] == "treatment",
            "Experiment branch must match"
        )
        XCTAssertNotNil(enrollmentEventExtras!["enrollment_id"], "Experiment enrollment id must not be nil")

        // Attempt to record an event for a non-existent or feature we are not enrolled in an
        // experiment in to ensure nothing is recorded.
        nimbus.recordExposureEvent(featureId: "not-a-feature")

        // Verify the invalid event was ignored by checking again that the valid event is still the only
        // event, and that it hasn't changed any of its extra properties.
        let enrollmentEventsTryTwo = try GleanMetrics.NimbusEvents.exposure.testGetValue()
        XCTAssertEqual(1, enrollmentEventsTryTwo.count, "Event count must match")
        let enrollmentEventExtrasTryTwo = enrollmentEventsTryTwo.first!.extra
        XCTAssertEqual("secure-gold", enrollmentEventExtrasTryTwo!["experiment"], "Experiment slug must match")
        XCTAssertTrue(
            enrollmentEventExtrasTryTwo!["branch"] == "control" || enrollmentEventExtrasTryTwo!["branch"] == "treatment",
            "Experiment branch must match"
        )
        XCTAssertNotNil(enrollmentEventExtrasTryTwo!["enrollment_id"], "Experiment enrollment id must not be nil")
    }

    func testRecordDisqualificationOnOptOut() throws {
        let appSettings = NimbusAppSettings(appName: "NimbusUnitTest", channel: "test")
        let nimbus = try Nimbus.create(nil, appSettings: appSettings, dbPath: createDatabasePath()) as! Nimbus

        // Load an experiment in nimbus that we will record an event in. The experiment bucket configuration
        // is set so that it will be guaranteed to be active. This is necessary because the SDK checks for
        // active experiments before recording.
        try nimbus.setExperimentsLocallyOnThisThread(minimalExperimentJSON())
        try nimbus.applyPendingExperimentsOnThisThread()

        // Assert that there are no events to start with
        XCTAssertFalse(GleanMetrics.NimbusEvents.exposure.testHasValue(), "Event must have a value")

        // Opt out of the experiment, which should generate a "disqualification" event
        try nimbus.optOutOnThisThread("secure-gold")

        // Use the Glean test API to check that the valid event is present
        XCTAssertTrue(GleanMetrics.NimbusEvents.disqualification.testHasValue(), "Event must have a value")
        let disqualificationEvents = try GleanMetrics.NimbusEvents.disqualification.testGetValue()
        XCTAssertEqual(1, disqualificationEvents.count, "Event count must match")
        let disqualificationEventExtras = disqualificationEvents.first!.extra
        XCTAssertEqual("secure-gold", disqualificationEventExtras!["experiment"], "Experiment slug must match")
        XCTAssertTrue(
            disqualificationEventExtras!["branch"] == "control" || disqualificationEventExtras!["branch"] == "treatment",
            "Experiment branch must match"
        )
        XCTAssertNotNil(disqualificationEventExtras!["enrollment_id"], "Experiment enrollment id must not be nil")
    }

    func testRecordDisqualificationOnGlobalOptOut() throws {
        let appSettings = NimbusAppSettings(appName: "NimbusUnitTest", channel: "test")
        let nimbus = try Nimbus.create(nil, appSettings: appSettings, dbPath: createDatabasePath()) as! Nimbus

        // Load an experiment in nimbus that we will record an event in. The experiment bucket configuration
        // is set so that it will be guaranteed to be active. This is necessary because the SDK checks for
        // active experiments before recording.
        try nimbus.setExperimentsLocallyOnThisThread(minimalExperimentJSON())
        try nimbus.applyPendingExperimentsOnThisThread()

        // Assert that there are no events to start with
        XCTAssertFalse(GleanMetrics.NimbusEvents.exposure.testHasValue(), "Event must have a value")

        // Opt out of all experiments, which should generate a "disqualification" event for the enrolled
        // experiment
        try nimbus.setGlobalUserParticipationOnThisThread(false)

        // Use the Glean test API to check that the valid event is present
        XCTAssertTrue(GleanMetrics.NimbusEvents.disqualification.testHasValue(), "Event must have a value")
        let disqualificationEvents = try GleanMetrics.NimbusEvents.disqualification.testGetValue()
        XCTAssertEqual(1, disqualificationEvents.count, "Event count must match")
        let disqualificationEventExtras = disqualificationEvents.first!.extra
        XCTAssertEqual("secure-gold", disqualificationEventExtras!["experiment"], "Experiment slug must match")
        XCTAssertTrue(
            disqualificationEventExtras!["branch"] == "control" || disqualificationEventExtras!["branch"] == "treatment",
            "Experiment branch must match"
        )
        XCTAssertNotNil(disqualificationEventExtras!["enrollment_id"], "Experiment enrollment id must not be nil")
    }
}

extension Device {
    static func isSimulator() -> Bool {
        return ProcessInfo.processInfo.environment["SIMULATOR_ROOT"] != nil
    }
}
