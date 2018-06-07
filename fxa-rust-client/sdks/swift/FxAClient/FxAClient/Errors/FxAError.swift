/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation

public enum FxAError: Error {
    case Unauthorized(message: String)
    case Unspecified(message: String)
    case Panic(message: String)

    // The name is attempting to indicate that we free fxaError.message if it
    // existed, and that it's a very bad idea to touch it after you call this
    // function
    static func fromConsuming(_ fxaError: FxAErrorC) -> FxAError? {
        let message = fxaError.message
        switch fxaError.code {
        case NoError:
            return nil
        case AuthenticationError:
            return .Unauthorized(message: String(freeingFxaString: message!))
        case Other:
            return .Unspecified(message: String(freeingFxaString: message!))
        case InternalPanic:
            return .Panic(message: String(freeingFxaString: message!))
        default:
            return .Unspecified(message: String(freeingFxaString: message!))
        }
    }

    @discardableResult
    public static func unwrap<T>(_ callback: (UnsafeMutablePointer<FxAErrorC>) throws -> T?) throws -> T {
        var err = FxAErrorC(code: NoError, message: nil)
        guard let result = try callback(&err) else {
            if let fxaErr = FxAError.fromConsuming(err) {
                throw fxaErr
            }
            throw ResultError.empty
        }
        return result
    }

    @discardableResult
    public static func tryUnwrap<T>(_ callback: (UnsafeMutablePointer<FxAErrorC>) throws -> T?) throws -> T? {
        var err = FxAErrorC(code: NoError, message: nil)
        guard let result = try callback(&err) else {
            if let fxaErr = FxAError.fromConsuming(err) {
                throw fxaErr
            }
            return nil
        }
        return result
    }
}
