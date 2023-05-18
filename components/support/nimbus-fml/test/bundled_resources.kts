/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import android.content.Context as MockContext
import com.example.bundling.MyNimbus
import com.example.app.R
import org.mozilla.experiments.nimbus.MockNimbus
import org.json.JSONObject

var injected: MockNimbus? = MockNimbus()
MyNimbus.initialize { injected }

val feature = MyNimbus.features.myStrings.value()

fun res(s: Int) = "res:$s"

assert(feature.scalar == res(R.string.my_scalar_text))
assert(feature.optional == res(R.string.my_optional_text))
assert(feature.optionalNil == null)
assert(feature.dictionary == mapOf("foo" to res(R.string.foo_text), "bar" to res(R.string.bar_text)))
assert(feature.list == listOf(res(R.string.foo_text), res(R.string.bar_text)))

// Make sure toJSONObject() works: Text, Optional<Text>, Map<String, Text>, List<Text>
val exp = JSONObject("""
    {
        "optional": "${res(R.string.my_optional_text)}",
        "scalar": "${res(R.string.my_scalar_text)}",
        "dictionary": {
            "foo": "${res(R.string.foo_text)}",
            "bar": "${res(R.string.bar_text)}"
        },
        "list": ["${res(R.string.foo_text)}", "${res(R.string.bar_text)}"]
    }
""".trimIndent())
if (exp.similar(feature.toJSONObject())) {
    assert(true)
} else {
    println("exp = ${exp}")
    println("obs = ${feature.toJSONObject()}")
    assert(false)
}

val feature1 = MyNimbus.features.myImages.value()
assert(feature1.scalar.resourceId == R.drawable.my_single_image)
assert(feature1.optional?.resourceId == R.drawable.my_optional_image)
assert(feature1.optionalNil == null)
assert(feature1.dictionary.mapValues { it.value.resourceId } == mapOf("foo" to R.drawable.foo_image, "bar" to R.drawable.bar_image))
assert(feature1.list.map { it.resourceId } == listOf(R.drawable.foo_image, R.drawable.bar_image))

// This has Image, optional Image, Map<String, Image>, List<Image>
val exp1 = JSONObject("""
    {
        "scalar": "${res(R.drawable.my_single_image)}",
        "optional": "${res(R.drawable.my_optional_image)}",
        "dictionary": {
            "foo": "${res(R.drawable.foo_image)}",
            "bar": "${res(R.drawable.bar_image)}"
        },
        "list": [
            "${res(R.drawable.foo_image)}",
            "${res(R.drawable.bar_image)}"
        ]
    }
""".trimIndent())
if (exp1.similar(feature1.toJSONObject())) {
    assert(true)
} else {
    println("exp = ${exp1}")
    println("obs = ${feature1.toJSONObject()}")
    assert(false)
}
