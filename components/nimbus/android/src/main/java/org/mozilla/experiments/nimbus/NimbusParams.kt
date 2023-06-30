/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.experiments.nimbus

import org.json.JSONObject
import java.util.Locale

/**
 * This class represents the client application name and channel for filtering purposes
 */
data class NimbusAppInfo(
    /**
     * The app name, used for experiment filtering purposes so that only the intended application
     * is targeted for the experiment.
     *
     * Examples: "fenix", "focus".
     *
     * For Mozilla products, this is defined in the telemetry system. For more context on where the
     * app_name comes for Mozilla products from see:
     * https://probeinfo.telemetry.mozilla.org/v2/glean/app-listings
     * and
     * https://github.com/mozilla/probe-scraper/blob/master/repositories.yaml
     */
    val appName: String,
    /**
     * The app channel used for experiment filtering purposes, so that only the intended application
     * channel is targeted for the experiment.
     *
     * Examples: "nightly", "beta", "release"
     */
    val channel: String,
    /**
     * Application derived attributes measured by the application, but useful for targeting of experiments.
     *
     * Example: mapOf("userType": "casual", "isFirstTime": "true")
     */
    val customTargetingAttributes: JSONObject = JSONObject(),
)

/**
 * Small struct for info derived from the device itself.
 */
data class NimbusDeviceInfo(
    val localeTag: String,
) {
    companion object {
        fun default() = NimbusDeviceInfo(Locale.getDefault().toLanguageTag())
    }
}
