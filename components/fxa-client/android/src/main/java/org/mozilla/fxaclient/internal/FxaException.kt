/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.fxaclient.internal

/**
 * Wrapper class for the exceptions thrown in the Rust library, which ensures that the
 * error messages will be consumed and freed properly in Rust.
 */
open class FxaException(message: String) : Exception(message) {
    class Unspecified(msg: String) : FxaException(msg)
    class Unauthorized(msg: String) : FxaException(msg)
    class Network(msg: String) : FxaException(msg)
    class Panic(msg: String) : FxaException(msg)

    companion object {
        // These numbers come from `ffi::error_codes` in the fxa-client rust code.
        private const val CODE_NETWORK: Int = 3
        private const val CODE_UNAUTHORIZED: Int = 2
        private const val CODE_PANIC: Int = -1
        internal fun fromConsuming(e: Error): FxaException {
            val message = e.consumeMessage() ?: ""
            return when (e.code) {
                CODE_UNAUTHORIZED -> Unauthorized(message)
                CODE_NETWORK -> Network(message)
                CODE_PANIC -> Panic(message)
                else -> Unspecified(message)
            }
        }
    }
}
