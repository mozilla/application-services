/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import android.content.Context as MockContext
import org.mozilla.experiments.nimbus.MockNimbus

var injected: MockNimbus? = null
MyNimbus.initialize { injected }

val feature = MyNimbus.features.nimbusValidation.value()

// Test the property level defaults.
assert(feature.enabled == true)
assert(feature.rowCount == 2)
assert(feature.deeplink == "deeplink://settings")
assert(feature.menuPosition == Position.BOTTOM)
assert(feature.enumMap[Position.TOP] == false)
assert(feature.enumMap[Position.BOTTOM] == true)

val api = MockNimbus("nimbus-validation" to """{
    "enabled": false,
    "row-count": 3,
    "deeplink": "deeplink://new-settings",
    "menu-position": "top",
    "enum-map": { "top": true, "bottom": false }
}""")
injected = api
MyNimbus.invalidateCachedValues()

// Completely override the defaults using the above JSON.
val feature1 = MyNimbus.features.nimbusValidation.value()
assert(feature1.enabled == false)
assert(feature1.rowCount == 3)
assert(feature1.deeplink == "deeplink://new-settings")
assert(feature1.menuPosition == Position.TOP)
assert(feature1.enumMap[Position.TOP] == true)
assert(feature1.enumMap[Position.BOTTOM] == false)

// Record exposure
MyNimbus.features.nimbusValidation.recordExposure()
assert(api.isExposed("nimbus-validation"))
