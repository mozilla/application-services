/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation

public enum FirefoxAccountError: Error {
    case Unauthorized(message: String)
    case Network(message: String)
    case Unspecified(message: String)
    case Panic(message: String)

    // The name is attempting to indicate that we free fxaError.message if it
    // existed, and that it's a very bad idea to touch it after you call this
    // function
    static func fromConsuming(_ rustError: FxAError) -> FirefoxAccountError? {
        let message = rustError.message
        switch rustError.code {
        case FxA_NoError:
            return nil
        case FxA_NetworkError:
            return .Network(message: String(freeingFxaString: message!))
        case FxA_AuthenticationError:
            return .Unauthorized(message: String(freeingFxaString: message!))
        case FxA_Other:
            return .Unspecified(message: String(freeingFxaString: message!))
        case FxA_InternalPanic:
            return .Panic(message: String(freeingFxaString: message!))
        default:
            return .Unspecified(message: String(freeingFxaString: message!))
        }
    }

    @discardableResult
    public static func unwrap<T>(_ callback: (UnsafeMutablePointer<FxAError>) throws -> T?) throws -> T {
        var err = FxAError(code: FxA_NoError, message: nil)
        let returnedVal = try callback(&err)
        if let fxaErr = FirefoxAccountError.fromConsuming(err) {
            throw fxaErr
        }
        guard let result = returnedVal else {
            throw ResultError.empty
        }
        return result
    }

    @discardableResult
    public static func tryUnwrap<T>(_ callback: (UnsafeMutablePointer<FxAError>) throws -> T?) throws -> T? {
        var err = FxAError(code: FxA_NoError, message: nil)
        let returnedVal = try callback(&err)
        if let fxaErr = FirefoxAccountError.fromConsuming(err) {
            throw fxaErr
        }
        return returnedVal
    }
}
