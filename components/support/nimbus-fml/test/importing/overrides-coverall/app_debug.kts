/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import org.mozilla.experiments.nimbus.MockNimbus
import com.example.app.nimbus.*
import com.example.lib.nimbus.*
import org.json.JSONObject

var injected: MockNimbus = MockNimbus(
    "property-overrides-test" to """{
        "variables-json": "variables-json"
    }"""
)

AppNimbus.initialize { injected }

val value = LibNimbus.features.overridesCoverall.value()
assert(value.noOverride == OverrideSource.NONE)
assert(value.scalar == OverrideSource.APP_FML)
assert(value.map[OverrideSource.APP_FML] == true)
assert(value.map[OverrideSource.CHANNEL_SPECIFIC] == true) // because we used release in the app fml.
assert(value.stringMap["app-fml"] == OverrideSource.APP_FML)
assert(value.nestedObject.scalar == OverrideSource.APP_FML)
assert(value.nestedObject.noOverride == OverrideSource.NONE)

// Make sure toJSONObject() works: Map<Enum, Boolean>, Map<String, Enum>, Object.
val obs = value.toJSONObject()
val exp = JSONObject("""
    {
        "no-override": "none",
        "scalar": "app-fml",
        "map": {
            "none": false,
            "lib-fml": true,
            "app-fml": true,
            "variables-json": false,
            "channel-specific": true
        },
        "string-map": {
            "none": "none",
            "lib-fml": "lib-fml",
            "app-fml": "app-fml",
            "variables-json": "none",
            "channel-specific": "channel-specific"
        },
        "nested-object": {
            "no-override": "none",
            "scalar": "app-fml"
        }
    }
""".trimIndent())
if (obs.similar(exp)) {
    assert(true)
} else {
    println("exp = ${exp}")
    println("obs = ${obs}")
    assert(false)
}
