/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation
import os.log
#if canImport(MozillaRustComponents)
    import MozillaRustComponents
#endif

/// Indicates an error occurred while calling into the places storage layer
extension PlacesError: LocalizedError {
    /// Our implementation of the localizedError protocol -- (This shows up in Sentry)
    public var errorDescription: String? {
        switch self {
        case let .UnexpectedPlacesException(message):
            return "PlacesError.UnexpectedPlacesException: \(message)"
        case let .InternalPanic(message):
            return "PlacesError.InternalPanic: \(message)"
        case let .InvalidParent(message):
            return "PlacesError.InvalidParent: \(message)"
        case let .UrlParseFailed(message):
            return "PlacesError.UrlParseFailed: \(message)"
        case let .PlacesConnectionBusy(message):
            return "PlacesError.PlacesConnectionBusy: \(message)"
        case let .OperationInterrupted(message):
            return "PlacesError.OperationInterrupted: \(message)"
        case let .BookmarksCorruption(message):
            return "PlacesError.BookmarksCorruption: \(message)"
        case let .InvalidParent(message):
            return "PlacesError.InvalidParent: \(message)"
        case let .UnknownBookmarkItem(message):
            return "PlacesError.UnknownBookmarkItem: \(message)"
        case let .UrlTooLong(message):
            return "PlacesError.UrlTooLong: \(message)"
        case let .InvalidBookmarkUpdate(message):
            return "PlacesError.InvalidBookmarkUpdate: \(message)"
        case let .CannotUpdateRoot(message):
            return "PlacesError.CannotUpdateRoot: \(message)"
        }
    }

    // The name is attempting to indicate that we free rustError.message if it
    // existed, and that it's a very bad idea to touch it after you call this
    // function
    static func fromConsuming(_ rustError: PlacesRustError) -> PlacesError? {
        let message = rustError.message == nil ? "" : String(freeingPlacesString: rustError.message!)
        return makeException(code: rustError.code, message: message)
    }

    static func makeException(code: PlacesErrorCode, message: String) -> PlacesError? {
        switch code {
        case Places_NoError:
            return nil
        case Places_UrlParseError:
            return .UrlParseFailed(message: message)
        case Places_DatabaseBusy:
            return .PlacesConnectionBusy(message: message)
        case Places_DatabaseInterrupted:
            return .OperationInterrupted(message: message)
        case Places_Corrupt:
            return .BookmarksCorruption(message: message)

        case Places_InvalidPlace_InvalidParent:
            return .InvalidParent(message: message)
        case Places_InvalidPlace_NoSuchItem:
            return .UnknownBookmarkItem(message: message)
        case Places_InvalidPlace_UrlTooLong:
            return .UrlTooLong(message: message)
        case Places_InvalidPlace_IllegalChange:
            return .InvalidBookmarkUpdate(message: message)
        case Places_InvalidPlace_CannotUpdateRoot:
            return .CannotUpdateRoot(message: message)

        case Places_Panic:
            return .panic(message: message)
        // Note: `1` is used as a generic catch all, but we
        // might as well handle the others the same way.
        default:
            return .unexpected(message: message)
        }
    }

    @discardableResult
    static func tryUnwrap<T>(_ callback: (UnsafeMutablePointer<PlacesRustError>) throws -> T?) throws -> T? {
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
    static func unwrap<T>(_ callback: (UnsafeMutablePointer<PlacesRustError>) throws -> T?) throws -> T {
        guard let result = try PlacesError.tryUnwrap(callback) else {
            throw PlacesError.UnexpectedPlacesException(message: "Unexpected error after unwrapping")
        }
        return result
    }

    // Same as `tryUnwrap`, but instead of erroring, just logs. Useful for cases like destructors where we
    // cannot throw.
    @discardableResult
    static func unwrapOrLog<T>(_ callback: (UnsafeMutablePointer<PlacesRustError>) throws -> T?) -> T? {
        do {
            let result = try PlacesError.tryUnwrap(callback)
            return result
        } catch let e {
            // Can't log what the error is without jumping through hoops apparently, oh well...
            os_log("Hit places error when throwing is impossible %{public}@", type: .error, "\(e)")
            return nil
        }
    }

    @discardableResult
    static func unwrapWithUniffi<T>(_ callback: (UnsafeMutablePointer<PlacesRustError>) throws -> T?) throws -> T? {
        do {
            var err = PlacesRustError(code: Places_NoError, message: nil)
            return try callback(&err)
        } catch let errorWrapper as ErrorWrapper {
            switch errorWrapper {
            case let .Wrapped(message):
                let splitError = message.components(separatedBy: "|")

                // If we couldn't get the right code, default to unexpected error
                let code = Int32(splitError[0]) ?? 1
                let message = splitError[1]
                throw makeException(code: PlacesErrorCode(code), message: message)!
            default:
                throw PlacesError.UnexpectedPlacesException(message: "Unexpected Error")
            }
        }
    }
}
