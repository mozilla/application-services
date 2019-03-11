/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation
import os.log

/// Indicates an error occurred while calling into the places storage layer
public enum PlacesError: Error {

    /// This indicates an attempt to use a connection after the PlacesApi
    /// it came from is destroyed. This indicates a usage error of this library.
    case ConnUseAfterApiClosed

    /// This is a catch-all error code used for errors not yet exposed to consumers,
    /// typically since it doesn't seem like there's a sane way for them to be handled.
    case Unexpected(message: String)

    /// The rust code implementing logins storage paniced. This always indicates a bug.
    case Panic(message: String)

    /// The place we were given is invalid.
    case InvalidPlace(message: String)

    /// We failed to parse the provided URL.
    case UrlParseError(message: String)

    /// The requested operation failed because the database was busy
    /// performing operations on a separate connection to the same DB.
    case DatabaseBusy(message: String)

    /// The requested operation failed because it was interrupted
    case DatabaseInterrupted(message: String)

    /// The requested operation failed because the store is corrupt
    case DatabaseCorrupt(message: String)

    // The name is attempting to indicate that we free rustError.message if it
    // existed, and that it's a very bad idea to touch it after you call this
    // function
    static func fromConsuming(_ rustError: PlacesRustError) -> PlacesError? {
        let message = rustError.message

        switch rustError.code {
        case Places_NoError:
            return nil

        case Places_Panic:
            return .Panic(message: String(freeingPlacesString: message!))

        case Places_UnexpectedError:
            return .Unexpected(message: String(freeingPlacesString: message!))

        case Places_InvalidPlaceInfo:
            return .InvalidPlace(message: String(freeingPlacesString: message!))

        case Places_UrlParseError:
            return .UrlParseError(message: String(freeingPlacesString: message!))

        case Places_DatabaseBusy:
            return .DatabaseBusy(message: String(freeingPlacesString: message!))

        case Places_DatabaseInterrupted:
            return .DatabaseInterrupted(message: String(freeingPlacesString: message!))

        case Places_Corrupt:
            return .DatabaseCorrupt(message: String(freeingPlacesString: message!))

        default:
            return .Unexpected(message: String(freeingPlacesString: message!))
        }
    }

    @discardableResult
    public static func tryUnwrap<T>(_ callback: (UnsafeMutablePointer<PlacesRustError>) throws -> T?) throws -> T? {
        var err = PlacesRustError(code: Places_NoError, message: nil)
        let returnedVal = try callback(&err)
        if let placesErr = PlacesError.fromConsuming(err) {
            throw placesErr
        }
        guard let result = returnedVal else {
            return nil
        }
        return result
    }

    @discardableResult
    public static func unwrap<T>(_ callback: (UnsafeMutablePointer<PlacesRustError>) throws -> T?) throws -> T {
        guard let result = try PlacesError.tryUnwrap(callback) else {
            throw ResultError.empty
        }
        return result
    }

    // Same as `tryUnwrap`, but instead of erroring, just logs. Useful for cases like destructors where we
    // cannot throw.
    @discardableResult
    public static func unwrapOrLog<T>(_ callback: (UnsafeMutablePointer<PlacesRustError>) throws -> T?) -> T? {
        do {
            let result = try PlacesError.tryUnwrap(callback)
            return result
        } catch let e {
            // Can't log what the error is without jumping through hoops apparently, oh well...
            os_log( "Hit places error when throwing is impossible %{public}@", type: .error, "\(e)")
            return nil
        }
    }
}
