/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import org.mozilla.experiments.nimbus.MockNimbus
import com.example.app.nimbus.*

var injected: MockNimbus = MockNimbus()

AppConfig.initialize { injected }

val value = UIConfig.features.appMenu.value()

// This shows that the including file overwrites the included file
assert(value.order == listOf("open-bookmarks", "open-logins", "open-pocket"))

// This shows that the menu items were contributed from both system and pocket.
assert(value.menuItems.keys == setOf("open-bookmarks", "open-logins", "open-pocket", "sync-pocket"))
