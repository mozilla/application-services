/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import FeatureManifest
import Foundation

let nimbus = MyNimbus.shared;

let feature = nimbus.features.myStrings.value()

assert(feature.scalar == "my_scalar_text")
assert(feature.optional == "my_optional_text")
assert(feature.optionalNil == nil)
assert(feature.dictionary == ["foo": "foo_text", "bar": "bar_text"])
assert(feature.list == ["foo_text", "bar_text"])

let feature1 = nimbus.features.myImages.value()
assert(feature1.scalar.name == "my_single_image")
assert(feature1.optional?.name == "my_optional_image")
assert(feature1.optionalNil == nil)
assert(feature1.dictionary.mapValues { $0.name } == ["foo": "foo_image", "bar": "bar_image"])
assert(feature1.list.map { $0.name } == ["foo_image", "bar_image"])