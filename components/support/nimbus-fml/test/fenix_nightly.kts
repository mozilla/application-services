/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import android.content.Context as MockContext
import com.example.app.R
import com.example.nightly.FxNimbus as MyNimbus
import com.example.nightly.HomeScreenSection
import org.mozilla.experiments.nimbus.MockNimbus

var injected: MockNimbus? = null
MyNimbus.initialize { injected }

// Test the default map with an enum to Boolean mapping based
// on the nightly defaults

val feature = MyNimbus.features.homescreen.value()
assert(feature.sectionsEnabled[HomeScreenSection.TOP_SITES] == true)
assert(feature.sectionsEnabled[HomeScreenSection.JUMP_BACK_IN] == true)
assert(feature.sectionsEnabled[HomeScreenSection.RECENTLY_SAVED] == true)
assert(feature.sectionsEnabled[HomeScreenSection.RECENT_EXPLORATIONS] == true)
assert(feature.sectionsEnabled[HomeScreenSection.POCKET] == true)

// Test whether we can selectively override the property based default.
val api = MockNimbus("homescreen" to """{
    "sections-enabled": {
        "pocket": false
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
assert(feature1.sectionsEnabled[HomeScreenSection.JUMP_BACK_IN] == true)
assert(feature1.sectionsEnabled[HomeScreenSection.RECENTLY_SAVED] == true)
assert(feature1.sectionsEnabled[HomeScreenSection.RECENT_EXPLORATIONS] == true)
assert(feature1.sectionsEnabled[HomeScreenSection.POCKET] == false)

// Record the exposure and test it.
MyNimbus.features.homescreen.recordExposure()
assert(api.isExposed("homescreen"))

val validationFeature = MyNimbus.features.nimbusValidation.value()
assert(validationFeature.settingsTitle == "hello")
assert(validationFeature.settingsPunctuation == "res:${R.string.app_menu_settings_punctuation}")
assert(validationFeature.settingsIcon.resourceId == R.drawable.mozac_ic_settings) { "Settings icon is ${validationFeature.settingsIcon.resourceId}" }
// Record the exposure and test it.
MyNimbus.features.nimbusValidation.recordExposure()
assert(api.isExposed("nimbus-validation"))

val searchTermGroupsFeature = MyNimbus.features.searchTermGroups.value()
assert(searchTermGroupsFeature.enabled == true)

MyNimbus.features.searchTermGroups.recordExposure()
assert(api.isExposed("search-term-groups"))
