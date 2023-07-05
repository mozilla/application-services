/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

@file:Suppress("InvalidPackageDeclaration")

package org.mozilla.experiments.nimbus

import android.content.Context
import org.json.JSONObject

class MockNimbus(override val context: Context, val map: Map<String, JSONObject>) : FeaturesInterface {

    constructor(vararg pairs: Pair<String, String>, context: Context = Context()) : this(
        context,
        pairs.toMap().mapValues { entry ->
            JSONObject(entry.value)
        },
    )

    init {
        NullVariables.instance.setContext(context)
    }

    override fun getVariables(featureId: String, recordExposureEvent: Boolean): Variables =
        map[featureId]?.let { json -> JSONVariables(context, json) } ?: NullVariables.instance

    private var exposureCounts = mutableMapOf<String, Int>()

    override fun recordExposureEvent(featureId: String, experimentSlug: String?) {
        if (map[featureId] != null) {
            exposureCounts[featureId] = getExposureCount(featureId) + 1
        }
    }

    fun getExposureCount(featureId: String) = exposureCounts.getOrDefault(featureId, 0)

    fun isExposed(featureId: String) = getExposureCount(featureId) > 0
}
