/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.experiments.nimbus

import android.content.Context
import org.json.JSONObject

class HardcodedNimbusFeatures(
    override val context: Context,
    private val features: Map<String, JSONObject>) : FeaturesInterface {
    private val exposureCounts = mutableMapOf<String, Int>()

    constructor(context: Context, vararg pairs: Pair<String, JSONObject>) : this(
        context,
        pairs.toMap()
    )

    init {
        NullVariables.instance.setContext(context)
    }

    override fun getVariables(featureId: String, recordExposureEvent: Boolean): Variables =
        features[featureId]?.let { json -> JSONVariables(context, json) } ?: NullVariables.instance

    override fun recordExposureEvent(featureId: String) {
        if (features[featureId] != null) {
            exposureCounts[featureId] = getExposureCount(featureId) + 1
        }
    }

    /**
     * Reports how many times the feature has had {recordExposureEvent} on it.
     */
    fun getExposureCount(featureId: String) = exposureCounts[featureId] ?: 0

    /**
     * Helper function for testing if the exposure count for this feature is greater
     * than zero.
     */
    fun isExposed(featureId: String) = getExposureCount(featureId) > 0

    fun hasFeature(featureId: String) = features.containsKey(featureId)
}
