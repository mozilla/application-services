/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

@file:Suppress("ktlint:standard:filename")

package org.mozilla.experiments.nimbus.internal

fun <K, V, K1> Map<K, V>.mapKeysNotNull(transform: (K) -> K1?): Map<K1, V> =
    this.entries
        .mapNotNull { e ->
            transform(e.key)?.let { it to e.value }
        }
        .toMap()

fun <K, V, V1> Map<K, V>.mapValuesNotNull(transform: (V) -> V1?): Map<K, V1> =
    this.entries
        .mapNotNull { e ->
            transform(e.value)?.let { e.key to it }
        }
        .toMap()

fun <K, V, K1, V1> Map<K, V>.mapEntriesNotNull(keyTransform: (K) -> K1?, valueTransform: (V) -> V1?): Map<K1, V1> =
    this.entries
        .mapNotNull { e ->
            val k1 = keyTransform(e.key) ?: return@mapNotNull null
            val v1 = valueTransform(e.value) ?: return@mapNotNull null
            k1 to v1
        }
        .toMap()

fun <K, V> Map<K, V>.mergeWith(defaults: Map<K, V>, valueTransform: ((V, V) -> V?)? = null) =
    valueTransform?.let {
        val target = mutableMapOf<K, V>()
        defaults.entries.forEach { entry ->
            target[entry.key] = entry.value
        }
        entries.forEach { entry ->
            val override = defaults[entry.key]
                ?.let { d -> valueTransform(entry.value, d) }
            target[entry.key] = override ?: entry.value
        }
        target
    } ?: defaults + this
