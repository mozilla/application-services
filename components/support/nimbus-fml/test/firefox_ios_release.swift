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
assert(feature.sectionsEnabled[HomeScreenSection.libraryShortcuts] == false)


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
    "search": """
    {
        "spotlight": {
            "enabled": true
        }
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
assert(feature1.sectionsEnabled[HomeScreenSection.libraryShortcuts] == false)


// Record the exposure and test it.
nimbus.features.homescreen.recordExposure()
assert(api.isExposed(featureId: "homescreen"))

let validationFeature = nimbus.features.nimbusValidation.value()
assert(validationFeature.settingsTitle == "hello")
assert(validationFeature.settingsTitlePunctuation == "")
assert(validationFeature.settingsIcon.name == "menu-Settings")
// Record the exposure and test it.
nimbus.features.nimbusValidation.recordExposure()
assert(api.isExposed(featureId: "nimbus-validation"))

let search = nimbus.features.search.value()
assert(search.spotlight.enabled == true)

nimbus.features.search.recordExposure()
assert(api.isExposed(featureId: "search"))
