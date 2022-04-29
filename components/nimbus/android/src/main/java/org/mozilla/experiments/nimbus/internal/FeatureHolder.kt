package org.mozilla.experiments.nimbus.internal

import org.mozilla.experiments.nimbus.FeaturesInterface
import org.mozilla.experiments.nimbus.Variables
import org.mozilla.experiments.nimbus.NullVariables
import java.util.concurrent.locks.ReentrantLock

/**
 * `FeatureHolder` is a class that unpacks a JSON object from the Nimbus SDK and transforms it into a useful
 * type safe object, generated from a feature manifest (a `.fml.yaml` file).
 *
 * The two routinely useful methods are the `value()` and `recordExposure()` events.
 *
 * There are methods useful for testing, and more advanced uses: these all start with `with`.
 */
class FeatureHolder<T>(
    private val getSdk: () -> FeaturesInterface?,
    private val featureId: String,
    private var create: (Variables) -> T
) {
    private val lock = ReentrantLock()

    private var cachedValue: T? = null

    /**
     * Get the JSON configuration from the Nimbus SDK and transform it into a configuration object as specified
     * in the feature manifest. This is done each call of the method, so the method should be called once, and the
     * result used for the configuration of the feature.
     *
     * @returns T
     * @throws NimbusFeatureException thrown before the Nimbus object has been constructed or `FxNimbus.initialize` has not been set.
     * This can be resolved by setting `FxNimbus.initialize`, and after that by passing in a `Context` object.
     */
    fun value(): T =
        lock.runBlock {
            if (cachedValue != null) {
                cachedValue!!
            } else {
                val variables = getSdk()?.getVariables(featureId, false) ?: NullVariables.instance
                create(variables).also { value ->
                    cachedValue = value
                }
            }
        }

    /**
     * Send an exposure event for this feature. This should be done when the user is shown the feature, and may change
     * their behavior because of it.
     */
    fun recordExposure() {
        getSdk()?.recordExposureEvent(featureId)
    }

    fun withCachedValue(value: T?) {
        lock.runBlock {
            cachedValue = value
        }
    }

    fun withInitializer(create: (Variables) -> T) {
        lock.runBlock {
            this.create = create
            this.cachedValue = null
        }
    }

    private fun <T> ReentrantLock.runBlock(block: () -> T): T {
        lock.lock()
        try {
            return block.invoke()
        } finally {
            lock.unlock()
        }
    }
}

class NimbusFeatureException(message: String) : Exception(message)
