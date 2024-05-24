/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import android.content.Context as MockContext
import com.example.app.R
import com.example.release.FxNimbus as MyNimbus
import com.example.release.HomeScreenSection
import org.mozilla.experiments.nimbus.MockNimbus

// Test the default map with an enum to Boolean mapping.
var injected: MockNimbus? = null
MyNimbus.initialize { injected }
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
}""", "nimbus-validation" to """{
    "settings-title": "hello"
}""", "search-term-groups" to """{
    "enabled": true
}""")

injected = api
MyNimbus.invalidateCachedValues()
val feature1 = MyNimbus.features.homescreen.value()
assert(feature1.sectionsEnabled[HomeScreenSection.TOP_SITES] == true)
assert(feature1.sectionsEnabled[HomeScreenSection.JUMP_BACK_IN] == false)
assert(feature1.sectionsEnabled[HomeScreenSection.RECENTLY_SAVED] == false)
assert(feature1.sectionsEnabled[HomeScreenSection.RECENT_EXPLORATIONS] == false)
assert(feature1.sectionsEnabled[HomeScreenSection.POCKET] == true)

// Record the exposure and test it.
MyNimbus.features.homescreen.recordExposure()
assert(api.isExposed("homescreen"))

val validationFeature = MyNimbus.features.nimbusValidation.value()
assert(validationFeature.settingsTitle == "hello")
assert(validationFeature.settingsPunctuation == "res:${R.string.app_menu_settings_punctuation}")
assert(validationFeature.settingsIcon.resourceId == R.drawable.mozac_ic_settings)
// Record the exposure and test it.
MyNimbus.features.nimbusValidation.recordExposure()
assert(api.isExposed("nimbus-validation"))

val searchTermGroupsFeature = MyNimbus.features.searchTermGroups.value()
assert(searchTermGroupsFeature.enabled == true)

MyNimbus.features.searchTermGroups.recordExposure()
assert(api.isExposed("search-term-groups"))
