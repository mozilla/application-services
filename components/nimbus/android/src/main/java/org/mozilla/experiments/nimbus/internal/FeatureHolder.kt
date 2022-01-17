package org.mozilla.experiments.nimbus.internal

import android.content.Context
import org.mozilla.experiments.nimbus.FeaturesInterface
import org.mozilla.experiments.nimbus.Variables
import org.mozilla.experiments.nimbus.NullVariables
import java.lang.ref.WeakReference

class FeatureHolder<T>(
    private val apiFn: () -> FeaturesInterface?,
    private val featureId: String,
    private val create: (Variables) -> T
) {
    private var exposureRecorder: (() -> Unit)? = null

    fun value(context: Context? = null): T {
        val api = apiFn()
        val ctx = context ?: api?.context ?: throw Exception("A Context is needed but not available. Consider passing in a context to the value() method when close to startup")
        val variables = api?.getVariables(featureId, false) ?: NullVariables(ctx)
        val feature = create(variables)
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
