/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import android.content.Context as MockContext
import com.example.app.R
import com.example.dx.DxNimbus as MyNimbus
import com.example.dx.TestEnum
import com.example.dx.TestObject
import org.mozilla.experiments.nimbus.MockNimbus
import org.json.JSONObject

val context = MockContext()
val nimbus = MockNimbus(context, mapOf())

val feature = MyNimbus.features.testFeature.value()

// A feature variable which is an enum map should have all variants represented
assert(feature.anEnumMap == mapOf(TestEnum.ALICE to 11, TestEnum.BOB to 22, TestEnum.CHARLIE to 33))

// An empty test object should have all variants represented
val obj = TestObject()
assert(obj.enumMap == mapOf(TestEnum.ALICE to 1, TestEnum.BOB to 2, TestEnum.CHARLIE to 3))

// The instance of test object which is specified in the feature has an overridden enum map.
assert(feature.anObject.enumMap == mapOf(TestEnum.ALICE to 11, TestEnum.BOB to 2, TestEnum.CHARLIE to 3)) { feature.anObject.enumMap }

// Make sure toJSONObject() works: Objects, Maps<Enum, Int>, Text, nested maps within objects.
val exp = JSONObject("""
    {
        "a-string": "A string",
        "an-enum-map": {
            "alice": 11,
            "bob": 22,
            "charlie": 33
        },
        "an-object": {
            "name": "Feature default",
            "enum-map": {
                "alice": 11,
                "bob": 2,
                "charlie": 3
            },
            "optional-int": 5,
            "style": "Not overwritten",
            "label": "${context.getString(R.string.dx_label)}"
        }
    }
""".trimIndent())
if (feature.toJSONObject().similar(exp)) {
    assert(true)
} else {
    println("exp = ${exp}")
    println("obs = ${feature.toJSONObject()}")
    assert(false)
}
