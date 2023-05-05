/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import FeatureManifest
import Foundation

let nimbus = MyNimbus.shared;

let feature = nimbus.features.homescreen.value()
assert(feature.sectionsEnabled[HomeScreenSection.topSites] == true)
assert(feature.sectionsEnabled[HomeScreenSection.jumpBackIn] == false)
assert(feature.sectionsEnabled[HomeScreenSection.recentlySaved] == false)
assert(feature.sectionsEnabled[HomeScreenSection.recentExplorations] == false)
assert(feature.sectionsEnabled[HomeScreenSection.pocket] == false)

// Test whether we can selectively override the property based default.
let api = HardcodedNimbusFeatures(with: [
    "homescreen": """
    {
        "sections-enabled": {
            "pocket": false
        }
    }
    """,
    "nimbus-validation": """
    {
        "settings-title": "hello"
    }
    """,
    "search-term-groups":  """
    {
        "enabled": true
    }
    """
])
nimbus.api = api
nimbus.invalidateCachedValues()
let feature1 = nimbus.features.homescreen.value()
assert(feature1.sectionsEnabled[HomeScreenSection.topSites] == true)
assert(feature1.sectionsEnabled[HomeScreenSection.jumpBackIn] == false)
assert(feature1.sectionsEnabled[HomeScreenSection.recentlySaved] == false)
assert(feature1.sectionsEnabled[HomeScreenSection.recentExplorations] == false)
assert(feature1.sectionsEnabled[HomeScreenSection.pocket] == false)

// Record the exposure and test it.
nimbus.features.homescreen.recordExposure()
assert(api.isExposed(featureId: "homescreen"))

let validationFeature = nimbus.features.nimbusValidation.value()
assert(validationFeature.settingsTitle == "hello")
assert(validationFeature.settingsPunctuation == "app_menu_settings_punctuation")
assert(validationFeature.settingsIcon.name == "mozac_ic_settings")
// Record the exposure and test it.
nimbus.features.nimbusValidation.recordExposure()
assert(api.isExposed(featureId: "nimbus-validation"))

let searchTermGroupsFeature = nimbus.features.searchTermGroups.value()
assert(searchTermGroupsFeature.enabled == true)

nimbus.features.searchTermGroups.recordExposure()
assert(api.isExposed(featureId: "search-term-groups"))
