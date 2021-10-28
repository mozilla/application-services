/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.experiments.nimbus.internal

public fun <K, V, K1> Map<K, V>.mapKeys(transform: (K) -> K1?): Map<K1, V> =
    this.entries
        .mapNotNull { e ->
            transform(e.key)?.let { it to e.value }
        }
        .toMap()

public fun <K, V, V1> Map<K, V>.mapValues(transform: (V) -> V1?): Map<K, V1> =
    this.entries
        .mapNotNull { e ->
            transform(e.value)?.let { e.key to it }
        }
        .toMap()

public fun <K, V, K1, V1> Map<K, V>.mapEntries(keyTransform: (K) -> K1?, valueTransform: (V) -> V1?) =
    this.entries
        .mapNotNull { e ->
            val k1 = keyTransform(e.key) ?: return@mapNotNull null
            val v1 = valueTransform(e.value) ?: return@mapNotNull null
            k1 to v1
        }
        .toMap()