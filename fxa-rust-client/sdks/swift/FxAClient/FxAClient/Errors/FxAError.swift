/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation

enum FxAError: Error {
    case Unauthorized(message: String)
    case Unspecified(message: String)

    static func from(rustError: RustError) -> FxAError {
        let message = String(cString: rustError.raw.pointee.message)
        switch rustError.raw.pointee.code {
        case AuthenticationError:
            return .Unauthorized(message: message)
        case Other:
            return .Unspecified(message: message)
        default:
            return .Unspecified(message: message)
        }
    }
}
