/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import org.mozilla.experiments.nimbus.MockNimbus
import com.example.app.nimbus.*
import com.example.lib.nimbus.*

val injected: MockNimbus = MockNimbus(
    "search" to """{
        "spotlight": {
            "keep-for-days": 42,
            "icon": "screenshot"
        }
    }""",
    "homescreen" to """{
        "sections-enabled": {
            "pocket": true
        }
    }"""
)

AppNimbus.initialize { injected }

// We have two different Nimbus features, in different parts of the app.
val search = AppNimbus.features.search.value()
assert(search.spotlight.keepForDays == 42)
assert(search.spotlight.icon == IconType.SCREENSHOT)

// Meanwhile in a different repository, in the same app.
val homescreen = LibNimbus.features.homescreen.value()
assert(homescreen.sectionsEnabled[HomeScreenSection.POCKET] == true)

// Show that the caching works by testing that the value that comes out of the
// feature holders triple equals the values we got out before.
assert(AppNimbus.features.search.value() === search)
assert(LibNimbus.features.homescreen.value() === homescreen)

// After calling the invaldiateCachedValues() method, triple equals
// should no longer holder.
AppNimbus.invalidateCachedValues()
assert(AppNimbus.features.search.value() !== search)
assert(LibNimbus.features.homescreen.value() !== homescreen)
