/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.viaduct

import mozilla.appservices.viaduct.initBackend
import mozilla.components.concept.fetch.Client
import mozilla.components.concept.fetch.MutableHeaders
import mozilla.components.concept.fetch.Request
import java.util.concurrent.TimeUnit
import kotlin.concurrent.atomics.AtomicBoolean
import kotlin.concurrent.atomics.ExperimentalAtomicApi
import mozilla.appservices.viaduct.allowAndroidEmulatorLoopback as rustAllowAndroidEmulatorLoopback

/**
 * All errors emitted by the client will subclass this.
 */
sealed class ViaductClientError(msg: String) : Exception(msg)

/**
 * Error indicating that the request method is not supported.
 */
class UnsupportedRequestMethodError(method: String) :
    ViaductClientError("Unsupported HTTP method: $method")

/**
 * Singleton allowing management of the HTTP backend
 * used by Rust components.
 */
@OptIn(ExperimentalAtomicApi::class)
object RustHttpConfig {
    /**
     * Set the HTTP client to be used by all Rust code.
     * the `Lazy`'s value is not read until the first request is made.
     */
    @Synchronized
    fun setClient(c: Lazy<Client>) {
        initBackend(FetchBackend(c))
    }

    /** Allows connections to the hard-coded address the Android Emulator uses
     * to connect to the emulator's host (ie, http://10.0.2.2) - if you don't
     * call this, viaduct will fail to use that address as it isn't https. The
     * expectation is that you will only call this in debug builds or if you
     * are sure you are running on an emulator.
     */
    fun allowAndroidEmulatorLoopback() {
        rustAllowAndroidEmulatorLoopback()
    }
}
