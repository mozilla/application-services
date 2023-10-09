/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import android.content.Context as MockContext
import org.json.JSONObject
import org.mozilla.experiments.nimbus.FeaturesInterface
import org.mozilla.experiments.nimbus.MockNimbus
import org.mozilla.experiments.nimbus.internal.FeatureHolder
import org.mozilla.experiments.nimbus.internal.FMLFeatureInterface
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

val scope = Executors.newWorkStealingPool(5)

val api: FeaturesInterface = MockNimbus("test-feature-holder" to "{}")

class Feature(val string: String): FMLFeatureInterface {
    override fun toJSONObject() =
        JSONObject(mapOf("string" to string))
}

val holder = FeatureHolder<Feature>({ api }, featureId = "test-feature-holder") { v, p -> Feature("NO CRASH") }

repeat(10000) {
    scope.submit {
        holder.value()
    }
}
repeat(2000) {
    scope.submit {
        holder.value()
    }
}

scope.shutdown()
scope.awaitTermination(2L, TimeUnit.SECONDS)

