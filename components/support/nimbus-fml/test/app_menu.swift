/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import FeatureManifest
import Foundation

// Exercise a map of booleans
let nimbus = MyNimbus.shared;
let feature = nimbus.features.appMenu.value()
assert(feature.itemEnabled[MenuItemId.startGame] == true)
assert(feature.itemEnabled[MenuItemId.resumeGame] == false)
assert(feature.itemEnabled[MenuItemId.settings] == true)
assert(feature.itemEnabled[MenuItemId.community] == false)

// Exercise a map of Objects.
assert(feature.items[MenuItemId.startGame]?.label == "Start Game")
assert(feature.items[MenuItemId.resumeGame]?.label == "Resume Game")
assert(feature.items[MenuItemId.settings]?.label == "Settings")
// This isn't in the map, though we might want to consider ensuring that
// every variant of the map is represented.
assert(feature.items[MenuItemId.community]?.label == "Community")

// Exercise a map of map of objects.
assert(feature.profileItems[PlayerProfile.child]![MenuItemId.startGame]?.label == "start child-friendly game")
assert(feature.profileItems[PlayerProfile.child]![MenuItemId.resumeGame]?.label == "resume child-friendly game")
assert(feature.profileItems[PlayerProfile.child]![MenuItemId.settings]?.label == "child-friendly tweaks")

assert(feature.profileItems[PlayerProfile.adult]![MenuItemId.startGame]?.label == "START")
assert(feature.profileItems[PlayerProfile.adult]![MenuItemId.resumeGame]?.label == "RESUME")
assert(feature.profileItems[PlayerProfile.adult]![MenuItemId.settings]?.label == "SETTINGS")

// Now let's merge it with JSON we might have got from Rust.
let api = HardcodedNimbusFeatures(with: ["app-menu":
"""
{
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
                "label": "NIMBUS settings",
            }
        }
    }
}
"""])
nimbus.api = api

nimbus.invalidateCachedValues()
let feature1 = nimbus.features.appMenu.value()
assert(feature1.items[MenuItemId.startGame]?.label == "Start Nimbus")
assert(feature1.items[MenuItemId.resumeGame]?.label == "Resume Nimbus")
assert(feature1.items[MenuItemId.settings]?.label == "Nimbus Settings")
assert(feature1.items[MenuItemId.community]?.label == "Share Nimbus")

assert(feature1.items[MenuItemId.startGame]?.deeplink == "deeplink://start")
assert(feature1.items[MenuItemId.resumeGame]?.deeplink == "deeplink://start?continue=true")
assert(feature1.items[MenuItemId.settings]?.deeplink == "deeplink://settings")
assert(feature1.items[MenuItemId.community]?.deeplink == "deeplink://community")

// Check that we're merging the maps properly.
assert(feature1.profileItems[PlayerProfile.child]![MenuItemId.startGame]?.label == "start child-friendly game")
assert(feature1.profileItems[PlayerProfile.child]![MenuItemId.resumeGame]?.label == "resume child-friendly game")
assert(feature1.profileItems[PlayerProfile.child]![MenuItemId.settings]?.label == "child-friendly tweaks")

assert(feature1.profileItems[PlayerProfile.adult]![MenuItemId.startGame]?.label == "START NIMBUS")
assert(feature1.profileItems[PlayerProfile.adult]![MenuItemId.resumeGame]?.label == "RESUME NIMBUS")
assert(feature1.profileItems[PlayerProfile.adult]![MenuItemId.settings]?.label == "NIMBUS settings")
