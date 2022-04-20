/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import FeatureManifest
import Foundation

let nimbus = MyNimbus.shared;
let feature = nimbus.features.testFeature.value()


// A feature variable which is an enum map should have all variants represented
assert(feature.anEnumMap == [TestEnum.alice:  11, TestEnum.bob: 22, TestEnum.charlie: 33])

// An empty test object should have all variants represented
let obj = TestObject()
assert(obj.enumMap == [TestEnum.alice: 1, TestEnum.bob: 2, TestEnum.charlie: 3])

// The instance of test object which is specified in the feature has an overridden enum map.
assert(feature.anObject.enumMap == [TestEnum.alice: 11, TestEnum.bob: 2, TestEnum.charlie: 3])

// Optional enums are handled properly and can default to null
assert(feature.anOptionalEnum == nil)

// Optional objects are handled properly and can default to null

assert(feature.anOptionalObject == nil)

// Optional enums can default to their variants properly
assert(feature.nonNullOptionalEnum == TestEnum.alice)

// Optional Object can default to a non-null object properly
assert(feature.nonNullOptionalObject!.optionalInt == 9)
