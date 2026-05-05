/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.experiments.nimbus.internal

import androidx.annotation.AnyThread
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import org.mozilla.experiments.nimbus.internal.EnrollmentChangeEvent
import java.util.concurrent.locks.ReentrantLock
import kotlin.concurrent.withLock

/**
 * The feature update dispatcher dispatches callbacks when feature
 * configurations change.
 */
class FeatureUpdateDispatcher(
    private val scope: CoroutineScope? = null,
 ) {
    private val lock = ReentrantLock()
    private val callbackMap: MutableMap<String, MutableSet<() -> Unit>> = mutableMapOf()

    /**
     * Register a callback to be called when the feature value changes.
     */
    @AnyThread
    public fun addCallback(featureId: String, callback: () -> Unit) {
        lock.withLock {
            callbackMap
                .getOrPut(featureId, { mutableSetOf<() -> Unit>() })
                .add(callback)
        }
    }

    /**
     * Remove a callback registration for a feature.
     */
    @AnyThread
    public fun removeCallback(featureId: String, callback: () -> Unit) {
        lock.withLock {
            callbackMap.get(featureId)?.run { remove(callback) }
        }
    }

    /**
     * Trigger the callbacks for all the features that have changed.
     */
    @AnyThread
    internal fun notifyChanged(events: List<EnrollmentChangeEvent>) {
        if (events.isEmpty()) {
            return
        }

        val featureIds = mutableSetOf<String>()

        for (event in events) {
            for (featureId in event.featureIds) {
                featureIds.add(featureId)
            }
        }

        notifyFeatures(featureIds)
    }

    /**
     * Trigger the callbacks for the given features.
     */
    @AnyThread
    internal fun notifyFeatures(featureIds: Set<String>) {
        val toUpdate = mutableSetOf<() -> Unit>()

        lock.withLock {
            for (featureId in featureIds) {
                callbackMap.get(featureId)?.also { callbacks ->
                    for (callback in callbacks) {
                        toUpdate.add(callback)
                    }
                }
            }
        }

        dispatch {
            for (callback in toUpdate) {
                callback()
            }
        }
    }

    /**
     * Dispatch a callback on the coroutine scope.
     *
     * If the scope is null, the callback will be invoked on this thread.
     */
    private fun dispatch(f: () -> Unit) {
        if (scope != null) {
            scope.launch {
                f()
            }
        } else {
            f()
        }
    }
}
