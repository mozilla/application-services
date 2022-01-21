// Test the default map with an enum to Boolean maping based
// on the nighlty defaults
import FeatureManifest
import Foundation

let feature = MyNimbus.features.homescreen.value()
assert(feature.sectionsEnabled[HomeScreenSection.topSites] == true)
assert(feature.sectionsEnabled[HomeScreenSection.jumpBackIn] == true)
assert(feature.sectionsEnabled[HomeScreenSection.recentlySaved] == true)
assert(feature.sectionsEnabled[HomeScreenSection.recentExplorations] == true)
assert(feature.sectionsEnabled[HomeScreenSection.pocket] == true)

// Test whether we can selectively override the property based default.
let api = MockNimbus(("homescreen", """
{
    "sections-enabled": {
        "pocket": false
    }
}
"""), ("nimbus-validation", """
{
    "settings-title": "hello"
}
"""), ("search-term-groups",  """
{
    "enabled": true
}
"""))
MyNimbus.api = api
let feature1 = MyNimbus.features.homescreen.value()
assert(feature1.sectionsEnabled[HomeScreenSection.topSites] == true)
assert(feature1.sectionsEnabled[HomeScreenSection.jumpBackIn] == true)
assert(feature1.sectionsEnabled[HomeScreenSection.recentlySaved] == true)
assert(feature1.sectionsEnabled[HomeScreenSection.recentExplorations] == true)
assert(feature1.sectionsEnabled[HomeScreenSection.pocket] == false)

// Record the exposure and test it.
MyNimbus.features.homescreen.recordExposure()
assert(api.isExposed(featureId: "homescreen"))

let validationFeature = MyNimbus.features.nimbusValidation.value()
assert(validationFeature.settingsTitle == "hello")
assert(validationFeature.settingsPunctuation == "")
assert(validationFeature.settingsIcon == "mozac_ic_settings")
// Record the exposure and test it.
MyNimbus.features.nimbusValidation.recordExposure()
assert(api.isExposed(featureId: "nimbus-validation"))

let searchTermGroupsFeature = MyNimbus.features.searchTermGroups.value()
assert(searchTermGroupsFeature.enabled == true)

MyNimbus.features.searchTermGroups.recordExposure()
assert(api.isExposed(featureId: "search-term-groups"))
