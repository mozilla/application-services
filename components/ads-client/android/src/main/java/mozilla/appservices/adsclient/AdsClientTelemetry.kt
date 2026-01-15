/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.adsclient

import mozilla.appservices.adsclient.MozAdsTelemetry
import org.mozilla.appservices.adsclient.GleanMetrics.AdsClient

/**
 * AdsClientTelemetry is a thin wrapper used to expose
 * callbacks used to emit telemetry events to Glean.
 */
class AdsClientTelemetry : MozAdsTelemetry {
    override fun recordBuildCacheError(label: String, value: String) {
        AdsClient.buildCacheError[label].set(value)
    }

    override fun recordClientError(label: String, value: String) {
        AdsClient.clientError[label].set(value)
    }

    override fun recordClientOperationTotal(label: String) {
        AdsClient.clientOperationTotal[label].add()
    }

    override fun recordDeserializationError(label: String, value: String) {
        AdsClient.deserializationError[label].set(value)
    }

    override fun recordHttpCacheOutcome(label: String, value: String) {
        AdsClient.httpCacheOutcome[label].set(value)
    }
}
