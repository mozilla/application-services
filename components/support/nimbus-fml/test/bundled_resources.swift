/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import FeatureManifest
import Foundation

let nimbus = MyNimbus.shared;

let feature = nimbus.features.myStrings.value()

assert(feature.scalar == "my-scalar-text")
assert(feature.optional == "my-optional-text")
assert(feature.optionalNil == nil)
assert(feature.dictionary == ["foo": "foo-text", "bar": "bar-text"])
assert(feature.list == ["foo-text", "bar-text"])

let feature1 = nimbus.features.myImages.value()
assert(feature1.scalar.name == "my-single-image")
assert(feature1.optional?.name == "my-optional-image")
assert(feature1.optionalNil == nil)
assert(feature1.dictionary.mapValues { $0.name } == ["foo": "foo-image", "bar": "bar-image"])
assert(feature1.list.map { $0.name } == ["foo-image", "bar-image"])