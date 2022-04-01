/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.experiments.nimbus

import org.json.JSONObject
import org.mozilla.experiments.nimbus.internal.NimbusStringHelperInterface
import org.mozilla.experiments.nimbus.internal.NimbusTargetingHelperInterface

/**
 * Instances of this class are useful for implementing a messaging service based upon
 * Nimbus.
 */
interface GleanPlumbInterface {
    fun createMessageHelper(additionalContext: JSONObject? = null): GleanPlumbMessageHelper =
        GleanPlumbMessageHelper(
            AlwaysFalseTargetingHelper(),
            NonStringHelper()
        )
}

/**
 * A helper object to make working with Strings uniform across multiple implementations of the messaging
 * system.
 *
 * This object provides access to a JEXL evaluator which runs against the same context as provided by
 * Nimbus targeting.
 *
 * It should also provide a similar function for String substitution, though this scheduled for EXP-2159.
 */
class GleanPlumbMessageHelper(
    private val targetingHelper: NimbusTargetingHelperInterface,
    private val stringHelper: NimbusStringHelperInterface
) : NimbusStringHelperInterface by stringHelper, NimbusTargetingHelperInterface by targetingHelper

internal class AlwaysFalseTargetingHelper : NimbusTargetingHelperInterface {
    override fun evalJexl(expression: String): Boolean = false
}

internal class NonStringHelper : NimbusStringHelperInterface {
    override fun stringFormat(
        template: String,
        uuid: String?
    ): String = template

    override fun getUuid(template: String) = null
}
