package nimbus.fml.test

import android.content.Context
import org.json.JSONObject
import org.mozilla.experiments.nimbus.JSONVariables
import org.mozilla.experiments.nimbus.Variables

private val context = Context()
object SmokeTestFeature {
    val variables = JSONVariables(context, JSONObject("""{ "string": "POW" }"""))

    val string: String by lazy {
        variables.getString("string") ?: "default"
    }
}