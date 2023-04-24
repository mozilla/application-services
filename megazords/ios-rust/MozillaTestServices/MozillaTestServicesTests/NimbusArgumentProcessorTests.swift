/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

@testable import MozillaTestServices

import XCTest

final class NimbusArgumentProcessorTests: XCTestCase {
    let unenrollExperiments = """
        {"data": []}
    """

    func testExperiments() throws {
        // This is an example of a functional test case.
        // Use XCTAssert and related functions to verify your tests produce the correct results.
        // Any test you write for XCTest can be annotated as throws and async.
        // Mark your test throws to produce an unexpected failure when your test encounters an uncaught error.
        // Mark your test async to allow awaiting for asynchronous code to complete. Check the results with assertions afterwards.
        XCTAssertNil(ArgumentProcessor.createCommandLineArgs(args: []))
        // No --nimbus-cli or --version 1
        XCTAssertNil(ArgumentProcessor.createCommandLineArgs(args: ["--experiments", "{\"data\": []}}"]))

        // No --version 1
        XCTAssertNil(ArgumentProcessor.createCommandLineArgs(args: ["--version", "1", "--experiments", "{\"data\": []}}"]))

        let argsUnenroll = ArgumentProcessor.createCommandLineArgs(args: ["--nimbus-cli", "--version", "1", "--experiments", unenrollExperiments])
        if let args = argsUnenroll {
            XCTAssertEqual(args.experiments, unenrollExperiments)
            XCTAssertFalse(args.resetDatabase)
        } else {
            XCTAssertNotNil(argsUnenroll)
        }
    }

    func testArgs() {
        XCTAssertEqual(
            ArgumentProcessor.createCommandLineArgs(args: ["--nimbus-cli", "--version", "1", "--experiments", unenrollExperiments]),
            CliArgs(resetDatabase: false, experiments: unenrollExperiments)
        )

        XCTAssertEqual(
            ArgumentProcessor.createCommandLineArgs(args: ["--nimbus-cli", "--version", "1", "--experiments", unenrollExperiments, "--reset-db"]),
            CliArgs(resetDatabase: true, experiments: unenrollExperiments)
        )

        XCTAssertEqual(
            ArgumentProcessor.createCommandLineArgs(args: ["--nimbus-cli", "--version", "1", "--reset-db"]),
            CliArgs(resetDatabase: true, experiments: nil)
        )
    }
}
