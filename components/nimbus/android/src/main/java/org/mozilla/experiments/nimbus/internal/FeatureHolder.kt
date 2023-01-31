/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.experiments.nimbus.internal

import android.content.Context
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
    private var getSdk: () -> FeaturesInterface?,
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
     * The unused parameter `Context` was relevant for `Text` and `Image` typed properties, but is no longer necessary.
     * It will be removed in later releases.
     *
     * @returns T
     * @throws NimbusFeatureException thrown before the Nimbus object has been constructed or `FxNimbus.initialize` has not been set.
     * This can be resolved by setting `FxNimbus.initialize`, and after that by passing in a `Context` object.
     */
    @Suppress("UNUSED_PARAMETER")
    fun value(context: Context? = null): T =
        lock.runBlock {
            if (cachedValue != null) {
                cachedValue!!
            } else {
                val variables = getSdk()?.getVariables(featureId, false) ?: run {
                    NullVariables.instance
                }
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

    /**
     * This overwrites the cached value with the passed one.
     *
     * This is most likely useful during testing only.
     */
    fun withCachedValue(value: T?) {
        lock.runBlock {
            cachedValue = value
        }
    }

    /**
     * This changes the mapping between a `Variables` and the feature configuration object.
     *
     * This is most likely useful during testing and other generated code.
     */
    fun withInitializer(create: (Variables) -> T) {
        lock.runBlock {
            this.create = create
            this.cachedValue = null
        }
    }

    /**
     * This resets the SDK and clears the cached value.
     *
     * This is especially useful at start up and for imported features.
     */
    fun withSdk(getSdk: () -> FeaturesInterface?) {
        lock.runBlock {
            this.getSdk = getSdk
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
