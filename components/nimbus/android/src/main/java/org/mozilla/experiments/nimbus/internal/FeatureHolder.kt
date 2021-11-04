package org.mozilla.experiments.nimbus.internal

import org.mozilla.experiments.nimbus.FeaturesInterface
import org.mozilla.experiments.nimbus.Variables
import java.lang.ref.SoftReference
import java.lang.ref.WeakReference
import java.util.concurrent.atomic.AtomicReference

class FeatureHolder<T>(private val apiFn: () -> FeaturesInterface?,
                       private val featureId: String,
                       private val create: (Variables?) -> T
) {
    private var exposureRecorder: (() -> Unit)? = null

    fun value(): T {
        val api = apiFn()
        val feature = create(api?.getVariables(featureId, false))
        api?.also { apiRef ->
            val weakRef = WeakReference(apiRef)
            exposureRecorder = {
                weakRef.get()?.recordExposureEvent(featureId)
            }
        }
        return feature
    }

    fun recordExposure() {
        exposureRecorder?.invoke()
    }
}