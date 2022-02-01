/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import XCTest

@testable import MozillaAppServices

class GleanPlumbTests: XCTestCase {
    func createDatabasePath() -> String {
        let directory = NSTemporaryDirectory()
        let filename = "testdb-\(UUID().uuidString).db"
        let fileURL = URL(fileURLWithPath: directory).appendingPathComponent(filename)
        return fileURL.absoluteString
    }

    func createNimbus() throws -> GleanPlumbProtocol {
        let appSettings = NimbusAppSettings(appName: "GleanPlumbTest", channel: "nightly")
        let nimbusEnabled = try Nimbus.create(nil, appSettings: appSettings, dbPath: createDatabasePath())
        XCTAssert(nimbusEnabled is Nimbus)
        if let nimbus = nimbusEnabled as? Nimbus {
            try nimbus.initializeOnThisThread()
        }
        return nimbusEnabled
    }

    func testJexlHelper() throws {
        let nimbus = try createNimbus()

        let helper = nimbus.createMessageHelper()
        XCTAssertTrue(try helper.evalJexl(expression: "app_name == 'GleanPlumbTest'"))
        XCTAssertFalse(try helper.evalJexl(expression: "app_name == 'tseTbmulPnaelG'"))

        XCTAssertThrowsError(try helper.evalJexl(expression: "appName == 'snake_case_only'"))
    }

    func testJexlHelperWithJson() throws {
        let nimbus = try createNimbus()

        let helper = nimbus.createMessageHelper()

        XCTAssertTrue(try helper.evalJexl(expression: "test_value_from_json == 42", json: ["test_value_from_json": 42]))

        let context = DummyContext(testValueFromJson: 42)
        XCTAssertTrue(try helper.evalJexl(expression: "test_value_from_json == 42", context: context))

        XCTAssertThrowsError(try helper.evalJexl(expression: "testValueFromJson == 42", context: context))
    }
}

private struct DummyContext: Encodable {
    let testValueFromJson: Int
}

private extension Device {
    static func isSimulator() -> Bool {
        return ProcessInfo.processInfo.environment["SIMULATOR_ROOT"] != nil
    }
}
