/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.experiments.nimbus

import android.content.Context
import org.json.JSONObject
import org.mozilla.experiments.nimbus.internal.FeatureManifestInterface

/**
 * Shim class for injecting JSON feature configs, as typed into the experimenter branch config page,
 * straight into the application.
 *
 * This is suitable for unit testing and ui testing.
 *
 * <code>
 *     val hardcodedNimbus = HardcodedNimbus(testContext,
 *          "my-feature" to JSONObject("""{
 *              "enabled": true
 *          }""".trimToIndent()
 *      )
 *     hardcodedNimbus.connectWith(FxNimbus)
 * </code>
 */
class HardcodedNimbusFeatures(
    override val context: Context,
    private val features: Map<String, JSONObject>,
) : FeaturesInterface {
    private val exposureCounts = mutableMapOf<String, Int>()
    private val malformedFeatures = mutableMapOf<String, String>()

    constructor(context: Context, vararg pairs: Pair<String, JSONObject>) : this(
        context,
        pairs.toMap(),
    )

    init {
        NullVariables.instance.setContext(context)
    }

    override fun getVariables(featureId: String, recordExposureEvent: Boolean): Variables =
        features[featureId]?.let { json ->
            if (recordExposureEvent) {
                recordExposureEvent(featureId)
            }
            JSONVariables(context, json)
        } ?: NullVariables.instance

    override fun recordExposureEvent(featureId: String, experimentSlug: String?) {
        if (features[featureId] != null) {
            exposureCounts[featureId] = getExposureCount(featureId) + 1
        }
    }

    override fun recordMalformedConfiguration(featureId: String, partId: String) {
        malformedFeatures[featureId] = partId
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

    /**
     * Utility function for {isUnderTest} to detect if the feature is under test.
     */
    fun hasFeature(featureId: String) = features.containsKey(featureId)

    /**
     * Helper function for testing if app code has reported that any of the feature
     * configuration is malformed.
     */
    fun isMalformed(featureId: String) =
        malformedFeatures[featureId] != null

    /**
     * Getter method for the last part of the given feature was reported malformed.
     */
    fun getMalformed(featureId: String) =
        malformedFeatures[featureId]

    /**
     * Use this {NimbusFeatures} instance to populate the passed feature configurations.
     */
    fun <T> connectWith(fm: FeatureManifestInterface<T>) {
        fm.initialize { this }
    }
}
