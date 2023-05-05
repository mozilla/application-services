/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import FeatureManifest
import Foundation

let injected = HardcodedNimbusFeatures()

AppNimbus.shared.initialize { injected }

let value = SubLibNimbus.shared.features.deeplyNestedFeature.value()
assert(value.noOverride == .none)
assert(value.libFml == .libFml)
assert(value.appFml == .appFml)
assert(value.orderDependent == .libFml)