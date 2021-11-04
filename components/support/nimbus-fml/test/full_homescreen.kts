/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import org.mozilla.experiments.nimbus.MockNimbus

val api = MockNimbus("homescreen" to """{
    "sections-enabled": {
    }
}""")
MyNimbus.api = api

val feature = MyNimbus.features.homescreen.value()


MyNimbus.features.homescreen.recordExposure()

assert(api.isExposed("homescreen"))


assert(feature.sectionsEnabled.isNotEmpty())