/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

@file:Suppress("InvalidPackageDeclaration")

package nimbus.fml.test

import android.content.Context
import org.json.JSONObject
import org.mozilla.experiments.nimbus.JSONVariables

private val context = Context()
object SmokeTestFeature {
    val variables = JSONVariables(context, JSONObject("""{ "string": "POW" }"""))

    val string: String by lazy {
        variables.getString("string") ?: "default"
    }
}
