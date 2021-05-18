/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import XCTest

@testable import MozillaAppServices

class NimbusFeatureVariablesTests: XCTestCase {
    func testScalarTypeCoercion() throws {
        let variables = JSONVariables(with: [
            "intVariable": 3,
            "stringVariable": "string",
            "booleanVariable": true,
        ])

        XCTAssertEqual(variables.getInt("intVariable"), 3)
        XCTAssertEqual(variables.getString("stringVariable"), "string")
        XCTAssertEqual(variables.getBool("booleanVariable"), true)
    }

    func testScalarValuesOfWrongTypeAreNil() throws {
        let variables = JSONVariables(with: [
            "intVariable": 3,
            "stringVariable": "string",
            "booleanVariable": true,
        ])
        XCTAssertNil(variables.getString("intVariable"))
        XCTAssertNil(variables.getBool("intVariable"))

        XCTAssertNil(variables.getInt("stringVariable"))
        XCTAssertNil(variables.getBool("stringVariable"))

        XCTAssertEqual(variables.getBool("booleanVariable"), true)
        XCTAssertNil(variables.getInt("booleanVariable"))
        XCTAssertNil(variables.getString("booleanVariable"))
    }

    func testNestedObjectsMakeVariablesObjects() throws {
        let outer = JSONVariables(with: [
            "inner": [
                "stringVariable": "string",
                "intVariable": 3,
                "booleanVariable": true,
            ],
            "really-a-string": "a string",
        ])

        XCTAssertNil(outer.getVariables("not-there"))
        let inner = outer.getVariables("inner")

        XCTAssertNotNil(inner)
        XCTAssertEqual(inner!.getInt("intVariable"), 3)
        XCTAssertEqual(inner!.getString("stringVariable"), "string")
        XCTAssertEqual(inner!.getBool("booleanVariable"), true)

        XCTAssertNil(outer.getVariables("really-a-string"))
    }
}
