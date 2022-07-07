/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import org.mozilla.experiments.nimbus.MockNimbus
import com.example.app.nimbus.*
import com.example.lib.nimbus.*

var injected: MockNimbus = MockNimbus(
    "property-overrides-test" to """{
        "variables-json": "variables-json"
    }"""
)

AppNimbus.initialize { injected }

val value = LibNimbus.features.propertyOverridesTest.value()
assert(value.noOverride == OverrideSource.NONE)
assert(value.channelSpecific == OverrideSource.CHANNEL_SPECIFIC)
assert(value.variablesJson == OverrideSource.VARIABLES_JSON)
