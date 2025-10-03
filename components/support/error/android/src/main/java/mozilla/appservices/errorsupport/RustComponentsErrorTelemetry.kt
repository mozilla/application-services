/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.errorsupport

import org.mozilla.appservices.errorsupport.GleanMetrics.Pings
import org.mozilla.appservices.errorsupport.GleanMetrics.RustComponentErrors

/**
 * RustErrorTelemetry forwarder
 *
 * This receives error pings from Rust and sends them to Glean.
 * It's only necessary because we currently can't call Glean directly from Rust
 */
public object RustComponentsErrorTelemetry {
    /**
     * Register the RustComponentsErrorTelemetry and start forwarding telemetry to glean
     */
    fun register() {
        registerErrorListener(KotlinErrorListener())
    }
}

private class KotlinErrorListener : ErrorListener {
    override fun onError(errorType: String, details: String, breadcrumbs: List<String>) {
        RustComponentErrors.errorType.set(errorType)
        RustComponentErrors.details.set(details)
        RustComponentErrors.breadcrumbs.set(breadcrumbs)
        Pings.rustComponentErrors.submit()
    }
}
