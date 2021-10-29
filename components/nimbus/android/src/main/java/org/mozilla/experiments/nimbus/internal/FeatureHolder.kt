package org.mozilla.experiments.nimbus.internal

import org.mozilla.experiments.nimbus.FeaturesInterface
import org.mozilla.experiments.nimbus.Variables

class FeatureHolder<T>(private val apiFn: () -> FeaturesInterface?, private val featureId: String, private val create: (Variables?) -> T) {

    fun value(): T = create(apiFn()?.getVariables(featureId, false))

    fun recordExposure() {
        apiFn()?.recordExposureEvent(featureId)
    }
}