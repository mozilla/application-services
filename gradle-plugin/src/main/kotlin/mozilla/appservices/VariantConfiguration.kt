/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices

/**
 * Captures configuration for a particular Android variant.
 */
data class VariantConfiguration(
    val name: String,
    var megazord: String? = null,
    var unitTestingEnabled: Boolean = true
)
