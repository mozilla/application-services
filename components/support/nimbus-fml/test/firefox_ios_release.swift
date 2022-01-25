// Test the default map with an enum to Boolean maping based
// on the nighlty defaults
import FeatureManifest
import Foundation

let feature = MyNimbus.features.homescreen.value()
assert(feature.sectionsEnabled[HomeScreenSection.topSites] == true)
assert(feature.sectionsEnabled[HomeScreenSection.jumpBackIn] == false)
assert(feature.sectionsEnabled[HomeScreenSection.recentlySaved] == false)
assert(feature.sectionsEnabled[HomeScreenSection.recentExplorations] == false)
assert(feature.sectionsEnabled[HomeScreenSection.pocket] == false)
assert(feature.sectionsEnabled[HomeScreenSection.libraryShortcuts] == false)


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
"""), ("search",  """
{
    "spotlight": {
        "enabled": true
    }
}
"""))
MyNimbus.api = api
let feature1 = MyNimbus.features.homescreen.value()
assert(feature1.sectionsEnabled[HomeScreenSection.topSites] == true)
assert(feature1.sectionsEnabled[HomeScreenSection.jumpBackIn] == false)
assert(feature1.sectionsEnabled[HomeScreenSection.recentlySaved] == false)
assert(feature1.sectionsEnabled[HomeScreenSection.recentExplorations] == false)
assert(feature1.sectionsEnabled[HomeScreenSection.pocket] == false)
assert(feature1.sectionsEnabled[HomeScreenSection.libraryShortcuts] == false)


// Record the exposure and test it.
MyNimbus.features.homescreen.recordExposure()
assert(api.isExposed(featureId: "homescreen"))

let validationFeature = MyNimbus.features.nimbusValidation.value()
assert(validationFeature.settingsTitle == "hello")
assert(validationFeature.settingsTitlePunctuation == "")
assert(validationFeature.settingsIcon == "menu-Settings")
// Record the exposure and test it.
MyNimbus.features.nimbusValidation.recordExposure()
assert(api.isExposed(featureId: "nimbus-validation"))

let search = MyNimbus.features.search.value()
assert(search.spotlight.enabled == true)

MyNimbus.features.search.recordExposure()
assert(api.isExposed(featureId: "search"))
