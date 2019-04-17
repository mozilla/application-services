/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation

/// Indicates an error occurred while calling into the logins storage layer
public enum LoginsStoreError: Error {

    /// This is a catch-all error code used for errors not yet exposed to consumers,
    /// typically since it doesn't seem like there's a sane way for them to be handled.
    case Unspecified(message: String)

    /// The rust code implementing logins storage paniced. This always indicates a bug.
    case Panic(message: String)

    /// This indicates that the sync authentication is invalid, likely due to having
    /// expired.
    case AuthInvalid(message: String)

    /// This is thrown if a `touch` or `update` refers to a record whose ID is not known
    case NoSuchRecord(message: String)

    /// This is thrown on attempts to `add` a record with a specific ID, but that ID
    /// already exists.
    case DuplicateGuid(message: String)

    /// This is thrown on attempts to insert or update a record so that it
    /// is no longer valid. Valid records have:
    ///
    /// - non-empty hostnames
    /// - non-empty passwords
    /// - and exactly one of `httpRealm` or `formSubmitUrl` is non-null.
    case InvalidLogin(message: String)

    /// This error is emitted in two cases:
    ///
    /// 1. An incorrect key is used to to open the login database
    /// 2. The file at the path specified is not a sqlite database.
    case InvalidKey(message: String)

    /// This error is emitted if a request to a sync server failed.
    case Network(message: String)

    /// This error is emitted if a call to `interrupt()` is made to
    /// abort some operation.
    case interrupted(message: String)

    // The name is attempting to indicate that we free rustError.message if it
    // existed, and that it's a very bad idea to touch it after you call this
    // function
    static func fromConsuming(_ rustError: Sync15PasswordsError) -> LoginsStoreError? {
        let message = rustError.message

        switch rustError.code {
        case Sync15Passwords_NoError:
            return nil

        case Sync15Passwords_OtherError:
            return .Unspecified(message: String(freeingRustString: message!))

        case Sync15Passwords_UnexpectedPanic:
            return .Panic(message: String(freeingRustString: message!))

        case Sync15Passwords_AuthInvalidError:
            return .AuthInvalid(message: String(freeingRustString: message!))

        case Sync15Passwords_NoSuchRecord:
            return .NoSuchRecord(message: String(freeingRustString: message!))

        case Sync15Passwords_DuplicateGuid:
            return .DuplicateGuid(message: String(freeingRustString: message!))

        case Sync15Passwords_InvalidLogin:
            return .InvalidLogin(message: String(freeingRustString: message!))

        case Sync15Passwords_InvalidKeyError:
            return .InvalidKey(message: String(freeingRustString: message!))

        case Sync15Passwords_NetworkError:
            return .Network(message: String(freeingRustString: message!))

        case Sync15Passwords_InterruptedError:
            return .interrupted(message: String(freeingRustString: message!))

        default:
            return .Unspecified(message: String(freeingRustString: message!))
        }
    }

    @discardableResult
    public static func unwrap<T>(_ callback: (UnsafeMutablePointer<Sync15PasswordsError>) throws -> T?) throws -> T {
        var err = Sync15PasswordsError(code: Sync15Passwords_NoError, message: nil)
        guard let result = try callback(&err) else {
            if let loginErr = LoginsStoreError.fromConsuming(err) {
                throw loginErr
            }
            throw ResultError.empty
        }
        // result might not be nil (e.g. it could be 0), while still indicating failure. Ultimately,
        // `err` is the source of truth here.
        if let loginErr = LoginsStoreError.fromConsuming(err) {
            throw loginErr
        }
        return result
    }

    @discardableResult
    public static func tryUnwrap<T>(_ callback: (UnsafeMutablePointer<Sync15PasswordsError>) throws -> T?) throws -> T? {
        var err = Sync15PasswordsError(code: Sync15Passwords_NoError, message: nil)
        guard let result = try callback(&err) else {
            if let loginErr = LoginsStoreError.fromConsuming(err) {
                throw loginErr
            }
            return nil
        }
        // result might not be nil (e.g. it could be 0), while still indicating failure. Ultimately,
        // `err` is the source of truth here.
        if let loginErr = LoginsStoreError.fromConsuming(err) {
            throw loginErr
        }
        return result
    }
}
