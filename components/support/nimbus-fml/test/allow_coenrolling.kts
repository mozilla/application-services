/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import com.example.app.nimbus.AppNimbus as MyNimbus

val coenrollingFeatureIds = MyNimbus.getCoenrollingFeatureIds()
assert(coenrollingFeatureIds == listOf("boring-app-menu", "fun-app-menu"))
