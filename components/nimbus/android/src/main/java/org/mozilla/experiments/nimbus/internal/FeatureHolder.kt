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

    /**
     * Get the JSON configuration from the Nimbus SDK and transform it into a configuration object as specified
     * in the feature manifest. This is done each call of the method, so the method should be called once, and the
     * result used for the configuration of the feature.
     *
     * An optional `Context` object is taken which is used to look up resources. Most of the time this isn't required, and the context can be
     * derived from the `Nimbus` singleton object. This is now deprecated, and will be removed in future releases.
     *
     * @returns T
     * @throws NimbusFeatureException thrown before the Nimbus object has been constructed or `FxNimbus.api` has not been set.
     * This can be resolved by setting `FxNimbus.api`, and after that by passing in a `Context` object.
     */
    @Suppress("UNUSED_PARAMETER")
    fun value(context: Context? = null): T {
        val api = apiFn()
        val variables = api?.getVariables(featureId, false) ?: NullVariables.instance
        val feature = create(variables)
        api?.also { apiRef ->
            val weakRef = WeakReference(apiRef)
            exposureRecorder = {
                weakRef.get()?.recordExposureEvent(featureId)
            }
        }
        return feature
    }

    /**
     * Send an exposure event for this feature. This should be done when the user is shown the feature, and may change
     * their behavior because of it.
     */
    fun recordExposure() {
        exposureRecorder?.invoke()
    }
}

class NimbusFeatureException(message: String) : Exception(message)
