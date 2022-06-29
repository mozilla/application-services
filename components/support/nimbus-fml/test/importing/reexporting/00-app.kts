/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import org.mozilla.experiments.nimbus.MockNimbus
import com.example.app.nimbus.*
import com.example.lib.nimbus.*
import com.example.sublib.nimbus.*

var injected: MockNimbus = MockNimbus()

AppNimbus.initialize { injected }

val value = SubLibNimbus.features.deeplyNestedFeature.value()
assert(value.noOverride == OverrideSource.NONE)
assert(value.libFml == OverrideSource.LIB_FML)
assert(value.appFml == OverrideSource.APP_FML)
assert(value.orderDependent == OverrideSource.APP_FML)
