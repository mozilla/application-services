/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
import XCTest
@testable import Logins

class LoginsTests: XCTestCase {

    override func setUp() {
        // This method is called before the invocation of each test method in the class.
    }

    override func tearDown() {
        // This method is called after the invocation of each test method in the class.
    }

    func testBadEncryptionKey() {
        let storage = LoginsStorage(databasePath: "test-rust-logins.db")
        var dbOpened = true
        do {
              try storage.unlock(withEncryptionKey: "foofoofoo")
        } catch {
            XCTFail("Failed to setup db")
        }

        try! storage.lock()

        do {
            try storage.unlock(withEncryptionKey: "zebra")
        } catch {
            dbOpened = false
        }

        XCTAssertFalse(dbOpened, "Bad key unlocked the db!")
    }
}
