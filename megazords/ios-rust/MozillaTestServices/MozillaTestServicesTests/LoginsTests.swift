/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

@testable import MozillaTestServices

import Glean
import XCTest

class LoginsTests: XCTestCase {
    var storage: LoginsStorage!

    override func setUp() {
        super.setUp()
        Glean.shared.resetGlean(clearStores: true)
    }

    override func tearDown() {
        // This method is called after the invocation of each test method in the class.
    }

    func testMigrationMetrics() throws {
        let json = """
            {"fixup_phase":{
                "num_processed":0,"num_succeeded":0,"num_failed":0,"total_duration":0,"errors":[]
            },
            "insert_phase":{"num_processed":0,"num_succeeded":0,"num_failed":0,"total_duration":0,"errors":[]
            },
            "num_processed":3,"num_succeeded":1,"num_failed":2,"total_duration":53,"errors":[
                "Invalid login: Login has illegal field: Origin is Malformed",
                "Invalid login: Origin is empty"
            ]}
        """

        recordMigrationMetrics(jsonString: json)
        XCTAssertEqual(3, try GleanMetrics.LoginsStoreMigration.numProcessed.testGetValue())
        XCTAssertEqual(2, try GleanMetrics.LoginsStoreMigration.numFailed.testGetValue())
        XCTAssertEqual(1, try GleanMetrics.LoginsStoreMigration.numSucceeded.testGetValue())
        XCTAssertEqual(53, try GleanMetrics.LoginsStoreMigration.totalDuration.testGetValue())

        // Note the truncation of the first error string.
        XCTAssertEqual(["Invalid login: Login has illegal field: Origin is ", "Invalid login: Origin is empty"], try GleanMetrics.LoginsStoreMigration.errors.testGetValue())
    }
}
