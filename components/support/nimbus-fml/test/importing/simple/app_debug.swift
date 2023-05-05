/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import FeatureManifest
import Foundation

let injected = HardcodedNimbusFeatures(with:
    [
        "search":
        """
        {
            "spotlight": {
                "keep-for-days": 42,
                "icon": "screenshot"
            }
        }
        """,
        "homescreen":
        """
        {
            "sections-enabled": {
                "pocket": true
            }
        }
        """
    ]
)

AppNimbus.shared.initialize { injected }

// We have two different Nimbus features, in different parts of the app.
let search = AppNimbus.shared.features.search.value()
assert(search.spotlight.keepForDays == 42)
assert(search.spotlight.icon == .screenshot)

// Meanwhile in a different repository, in the same app.
let homescreen = LibNimbus.shared.features.homescreen.value()
assert(homescreen.sectionsEnabled[.pocket] == true)

// Show that the caching works by testing that the value that comes out of the
// feature holders triple equals the values we got out before.
assert(AppNimbus.shared.features.search.value() === search)
assert(LibNimbus.shared.features.homescreen.value() === homescreen)

// After calling the invaldiateCachedValues() method, triple equals
// should no longer holder.
AppNimbus.shared.invalidateCachedValues()
assert(AppNimbus.shared.features.search.value() !== search)
assert(LibNimbus.shared.features.homescreen.value() !== homescreen)
