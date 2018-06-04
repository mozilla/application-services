/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation

extension Result {
    /**
     Force unwraps a result.
     Expects there to be a value attached and throws an error is there is not.

     - Throws: `ResultError.error` if the result contains an error
     - Throws: `ResultError.empty` if the result contains no error but also no result.

     - Returns: The pointer to the successful result value.
     */
    @discardableResult public func unwrap() throws -> OpaquePointer {
        guard let success = self.ok else {
            if let error = self.err {
                throw FxAError.from(rustError: RustError(raw: error))
            }
            throw ResultError.empty
        }
        return OpaquePointer(success)
    }

    @discardableResult public func unwrap<T>() throws -> UnsafeMutablePointer<T> {
        guard let success = self.ok else {
            if let error = self.err {
                throw FxAError.from(rustError: RustError(raw: error))
            }
            throw ResultError.empty
        }
        return success.assumingMemoryBound(to: T.self)
    }

    /**
     Unwraps an optional result, yielding either a successful value or a nil.

     - Throws: `ResultError.error` if the result contains an error

     - Returns: The pointer to the successful result value, or nil if no value is present.
     */
    @discardableResult public func tryUnwrap<T>() throws -> UnsafeMutablePointer<T>? {
        if let error = self.err {
            throw FxAError.from(rustError: RustError(raw: error))
        }
        return self.ok?.assumingMemoryBound(to: T.self)
    }
}
