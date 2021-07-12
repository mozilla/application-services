/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
// import XCTest

// @testable import MozillaAppServices

// class LoginsTests: XCTestCase {
//     var storage: LoginsStorage!
//     var encryptionKey: String!
//     var salt: String!

//     // This test setup mimics as close as we can to how fxiOS consumes our API
//     override func setUp() {
//         let directory = NSTemporaryDirectory()
//         let filename = "testdb-\(UUID().uuidString).db"
//         let fileURL = URL(fileURLWithPath: directory).appendingPathComponent(filename)
//         let databasePath = URL(fileURLWithPath: fileURL.absoluteString).absoluteString

//         // Note: SQLite supports using file: urls, so this works. (Maybe we should allow
//         // passing in a URL argument too?)
//         encryptionKey = "uyvuyvuvWSRYRWYRW47654754hdxjkinouhi"
//         salt = setupPlaintextHeaderAndGetSalt(databasePath: databasePath, encryptionKey: encryptionKey)
//         storage = LoginsStorage(databasePath: fileURL.absoluteString)
//     }

//     override func tearDown() {
//         // This method is called after the invocation of each test method in the class.
//     }

//     // Migrate and return the salt, or create a new salt
//     // Also, in the event of an error, returns a new salt.
//     public func setupPlaintextHeaderAndGetSalt(databasePath: String, encryptionKey: String) -> String {
//         do {
//             if FileManager.default.fileExists(atPath: databasePath) {
//                 let db = LoginsStorage(databasePath: databasePath)
//                 let salt = try db.getDbSaltForKey(key: encryptionKey)
//                 try db.migrateToPlaintextHeader(key: encryptionKey, salt: salt)
//                 return salt
//             }
//         } catch {
//             print("could not sucessfully migrate plaintext")
//         }
//         let saltOf32Chars = UUID().uuidString.replacingOccurrences(of: "-", with: "")
//         return saltOf32Chars
//     }

//     func testBadEncryptionKey() {
//         var dbOpened = true
//         do {
//             try storage.unlockWithKeyAndSalt(key: encryptionKey, salt: salt)
//         } catch {
//             XCTFail("Failed to setup db")
//         }

//         try! storage.lock()

//         do {
//             try storage.unlockWithKeyAndSalt(key: "zebra", salt: salt)
//         } catch {
//             dbOpened = false
//         }

//         XCTAssertFalse(dbOpened, "Bad key unlocked the db!")
//     }

//     func testLoginNil() {
//         try! storage.unlockWithKeyAndSalt(key: encryptionKey, salt: salt)
//         let id0 = try! storage.add(login: Login(
//             id: "",
//             hostname: "https://www.example.com",
//             password: "hunter2",
//             username: "cooluser33",
//             httpRealm: nil,
//             formSubmitUrl: "https://www.example.com/login",
//             usernameField: "users_name",
//             passwordField: "users_password",
//             timesUsed: 0,
//             timeCreated: 0,
//             timeLastUsed: 0,
//             timePasswordChanged: 0
//         ))

//         let record0 = try! storage.get(id: id0)!
//         XCTAssertNil(record0.httpRealm)
//         // We fixed up the formSubmitUrl to just be the origin part of the url.
//         XCTAssertEqual(record0.formSubmitUrl, "https://www.example.com")

//         let id1 = try! storage.add(login: Login(
//             id: "",
//             hostname: "https://www.example2.com",
//             password: "hunter3",
//             username: "cooluser44",
//             httpRealm: "Something Something",
//             formSubmitUrl: nil,
//             usernameField: "",
//             passwordField: "",
//             timesUsed: 0,
//             timeCreated: 0,
//             timeLastUsed: 0,
//             timePasswordChanged: 0
//         ))

//         let record1 = try! storage.get(id: id1)!

//         XCTAssertNil(record1.formSubmitUrl)
//         XCTAssertEqual(record1.httpRealm, "Something Something")
//     }

//     func testLoginEnsureValid() {
//         try! storage.unlockWithKeyAndSalt(key: encryptionKey, salt: salt)

//         _ = try! storage.add(login: Login(
//             id: "",
//             hostname: "https://www.example5.com",
//             password: "hunter5",
//             username: "cooluser55",
//             httpRealm: nil,
//             formSubmitUrl: "https://www.example5.com",
//             usernameField: "users_name",
//             passwordField: "users_password",
//             timesUsed: 0,
//             timeCreated: 0,
//             timeLastUsed: 0,
//             timePasswordChanged: 0
//         ))

//         let dupeLogin = Login(
//             id: "",
//             hostname: "https://www.example5.com",
//             password: "hunter3",
//             username: "cooluser55",
//             httpRealm: nil,
//             formSubmitUrl: "https://www.example5.com",
//             usernameField: "users_name",
//             passwordField: "users_password",
//             timesUsed: 0,
//             timeCreated: 0,
//             timeLastUsed: 0,
//             timePasswordChanged: 0
//         )

//         let nullValueLogin = Login(
//             id: "",
//             hostname: "https://www.example6.com",
//             password: "hunter3",
//             username: "\0cooluser56",
//             httpRealm: nil,
//             formSubmitUrl: "https://www.example6.com",
//             usernameField: "users_name",
//             passwordField: "users_password",
//             timesUsed: 0,
//             timeCreated: 0,
//             timeLastUsed: 0,
//             timePasswordChanged: 0
//         )

//         XCTAssertThrowsError(try storage.ensureValid(login: dupeLogin))
//         XCTAssertThrowsError(try storage.ensureValid(login: nullValueLogin))
//     }

//     func addLogin() -> String {
//         let login = Login(
//             id: "",
//             hostname: "https://www.example5.com",
//             password: "hunter3",
//             username: "cooluser55",
//             httpRealm: nil,
//             formSubmitUrl: "https://www.example5.com",
//             usernameField: "users_name",
//             passwordField: "users_password",
//             timesUsed: 0,
//             timeCreated: 0,
//             timeLastUsed: 0,
//             timePasswordChanged: 0
//         )
//         return try! storage.add(login: login)
//     }

//     func testListLogins() {
//         try! storage.unlockWithKeyAndSalt(key: encryptionKey, salt: salt)

//         let listResult1 = try! storage.list()
//         XCTAssertEqual(listResult1.count, 0)

//         _ = addLogin()

//         let listResult2 = try! storage.list()
//         XCTAssertEqual(listResult2.count, 1)
//     }
// }
