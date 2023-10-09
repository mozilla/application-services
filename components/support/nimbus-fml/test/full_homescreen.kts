/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import android.content.Context as MockContext
import org.mozilla.experiments.nimbus.MockNimbus
import org.mozilla.experiments.nimbus.internal.FeatureHolder

// Test the default map with an enum to Boolean maping.
val feature = MyNimbus.features.homescreen.value()
assert(feature.sectionsEnabled[HomeScreenSection.TOP_SITES] == true)
assert(feature.sectionsEnabled[HomeScreenSection.JUMP_BACK_IN] == false)
assert(feature.sectionsEnabled[HomeScreenSection.RECENTLY_SAVED] == false)
assert(feature.sectionsEnabled[HomeScreenSection.RECENT_EXPLORATIONS] == false)
assert(feature.sectionsEnabled[HomeScreenSection.POCKET] == false)

// Test whether we can selectively override the property based default.
val api = MockNimbus("homescreen" to """{
    "sections-enabled": {
        "pocket": true
    }
}""")
val holder = FeatureHolder(getSdk = { api }, featureId = "homescreen") { v, _ -> Homescreen(v) }
val feature1 = holder.value()
assert(feature1.sectionsEnabled[HomeScreenSection.TOP_SITES] == true)
assert(feature1.sectionsEnabled[HomeScreenSection.JUMP_BACK_IN] == false)
assert(feature1.sectionsEnabled[HomeScreenSection.RECENTLY_SAVED] == false)
assert(feature1.sectionsEnabled[HomeScreenSection.RECENT_EXPLORATIONS] == false)
assert(feature1.sectionsEnabled[HomeScreenSection.POCKET] == true)

// Record the exposure and test it.
holder.recordExposure()
assert(api.isExposed("homescreen"))
