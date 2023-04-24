/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import FeatureManifest
import Foundation

// We must call AppConfig to link AppConfig and UIConfig together.
let injected: FeaturesInterface = HardcodedNimbusFeatures()
AppConfig.shared.initialize { injected }

// By now, UI config should have all the things from system and pocket.
let value = UIConfig.shared.features.appMenu.value()

// This shows that the including file overwrites the included file
assert(value.order == ["open-bookmarks", "open-logins", "open-pocket"])

// This shows that the menu items were contributed from both system and pocket.
let observed = value.menuItems.keys
let expected = ["open-bookmarks", "open-logins", "open-pocket", "sync-pocket"]
assert(Set(observed) == Set(expected))
