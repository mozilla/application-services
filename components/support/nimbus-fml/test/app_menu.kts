/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import android.content.Context as MockContext
import org.mozilla.experiments.nimbus.MockNimbus

// Exercise a map of booleans
val feature = MyNimbus.features.appMenu.value(MockContext())
assert(feature.itemEnabled[MenuItemId.START_GAME] == true)
assert(feature.itemEnabled[MenuItemId.RESUME_GAME] == false)
assert(feature.itemEnabled[MenuItemId.SETTINGS] == true)
assert(feature.itemEnabled[MenuItemId.COMMUNITY] == false)

// Excercise a map of Objects.
assert(feature.items[MenuItemId.START_GAME]?.label == "Start Game")
assert(feature.items[MenuItemId.RESUME_GAME]?.label == "Resume Game")
assert(feature.items[MenuItemId.SETTINGS]?.label == "Settings")
// This isn't in the map, though we might want to consider ensuring that
// every variant of the map is represented.
assert(feature.items[MenuItemId.COMMUNITY]?.label == "Community")

// Exercise a map of map of objects.
assert(feature.profileItems[PlayerProfile.CHILD]!![MenuItemId.START_GAME]?.label == "start child-friendly game")
assert(feature.profileItems[PlayerProfile.CHILD]!![MenuItemId.RESUME_GAME]?.label == "resume child-friendly game")
assert(feature.profileItems[PlayerProfile.CHILD]!![MenuItemId.SETTINGS]?.label == "child-friendly tweaks")

assert(feature.profileItems[PlayerProfile.ADULT]!![MenuItemId.START_GAME]?.label == "START")
assert(feature.profileItems[PlayerProfile.ADULT]!![MenuItemId.RESUME_GAME]?.label == "RESUME")
assert(feature.profileItems[PlayerProfile.ADULT]!![MenuItemId.SETTINGS]?.label == "SETTINGS")

// Now let's merge it with JSON we might have got from Rust.
MyNimbus.api = MockNimbus("app-menu" to """{
    "items": {
        "start-game": {
            "label": "Start Nimbus",
        },
        "resume-game": {
            "label": "Resume Nimbus",
        },
        "settings": {
            "label": "Nimbus Settings",
        },
        "community": {
            "label": "Share Nimbus"
        }
    },
    "profile-items": {
        "adult": {
            "start-game": {
                "label": "START NIMBUS",
            },
            "resume-game": {
                "label": "RESUME NIMBUS",
            },
            "settings": {
                "label": "NIMBUS SETTINGS",
            }
        }
    }
}""")

val feature1 = MyNimbus.features.appMenu.value()
assert(feature1.items[MenuItemId.START_GAME]?.label == "Start Nimbus")
assert(feature1.items[MenuItemId.RESUME_GAME]?.label == "Resume Nimbus")
assert(feature1.items[MenuItemId.SETTINGS]?.label == "Nimbus Settings")
assert(feature1.items[MenuItemId.COMMUNITY]?.label == "Share Nimbus")

assert(feature1.items[MenuItemId.START_GAME]?.deeplink == "deeplink://start")
assert(feature1.items[MenuItemId.RESUME_GAME]?.deeplink == "deeplink://start?continue=true")
assert(feature1.items[MenuItemId.SETTINGS]?.deeplink == "deeplink://settings")
assert(feature1.items[MenuItemId.COMMUNITY]?.deeplink == "deeplink://community")

// Check that we're merging the maps properly.
assert(feature1.profileItems[PlayerProfile.CHILD]!![MenuItemId.START_GAME]?.label == "start child-friendly game")
assert(feature1.profileItems[PlayerProfile.CHILD]!![MenuItemId.RESUME_GAME]?.label == "resume child-friendly game")
assert(feature1.profileItems[PlayerProfile.CHILD]!![MenuItemId.SETTINGS]?.label == "child-friendly tweaks")

assert(feature1.profileItems[PlayerProfile.ADULT]!![MenuItemId.START_GAME]?.label == "START NIMBUS")
assert(feature1.profileItems[PlayerProfile.ADULT]!![MenuItemId.RESUME_GAME]?.label == "RESUME NIMBUS")
assert(feature1.profileItems[PlayerProfile.ADULT]!![MenuItemId.SETTINGS]?.label == "NIMBUS SETTINGS")
