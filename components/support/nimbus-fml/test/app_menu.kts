/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import android.content.Context as MockContext
import org.mozilla.experiments.nimbus.MockNimbus

var injected: MockNimbus? = null
MyNimbus.initialize { injected }

// Exercise a map of booleans
val feature = MyNimbus.features.appMenu.value()
assert(feature.itemEnabled[MenuItemId.START_GAME] == true)
assert(feature.itemEnabled[MenuItemId.RESUME_GAME] == false)
assert(feature.itemEnabled[MenuItemId.SETTINGS] == true)
assert(feature.itemEnabled[MenuItemId.COMMUNITY] == false)

// Exercise a map of Objects.
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
injected = MockNimbus("app-menu" to """{
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
MyNimbus.invalidateCachedValues()

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

val exp = org.json.JSONObject("""
{
	"item-enabled": {
		"start-game": true,
		"resume-game": false,
		"settings": true,
		"community": false
	},
	"profile-ordering": [{
			"adult": "settings",
			"child": "start-game"
		},
		{
			"adult": "settings",
			"child": "resume-game"
		}
	],
	"all-menu-items": [{
			"label": "Start Game",
			"deeplink": "deeplink://start"
		},
		{
			"label": "Resume Game",
			"deeplink": "deeplink://start?continue=true"
		}
	],
	"item-ordering": [
		"resume-game",
		"start-game",
		"community",
		"settings"
	],
	"profile-items": {
		"adult": {
			"start-game": {
				"label": "START NIMBUS",
				"deeplink": "deeplink://start"
			},
			"resume-game": {
				"label": "RESUME NIMBUS",
				"deeplink": "deeplink://start?continue=true"
			},
			"settings": {
				"label": "NIMBUS SETTINGS",
				"deeplink": "deeplink://settings"
			},
			"community": {
				"label": "COMMUNITY",
				"deeplink": "deeplink://community"
			}
		},
		"child": {
			"start-game": {
				"label": "start child-friendly game",
				"deeplink": "deeplink://start"
			},
			"resume-game": {
				"label": "resume child-friendly game",
				"deeplink": "deeplink://start?continue=true"
			},
			"settings": {
				"label": "child-friendly tweaks",
				"deeplink": "deeplink://settings"
			},
			"community": {
				"label": "child-friendly community engagement!",
				"deeplink": "deeplink://community"
			}
		}
	},
	"items": {
		"start-game": {
			"label": "Start Nimbus",
			"deeplink": "deeplink://start"
		},
		"resume-game": {
			"label": "Resume Nimbus",
			"deeplink": "deeplink://start?continue=true"
		},
		"settings": {
			"label": "Nimbus Settings",
			"deeplink": "deeplink://settings"
		},
		"community": {
			"label": "Share Nimbus",
			"deeplink": "deeplink://community"
		}
	}
}
""".trimIndent())
val obs = feature1.toJSONObject()
if (exp.similar(obs)) {
    assert(true)
} else {
    println("exp = $exp")
    println("obs = $obs")
    assert(false)
}
