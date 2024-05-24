/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import FeatureManifest
import Foundation

let nimbus = MyNimbus.shared

let feature = nimbus.features.withObjectsFeature.value()

// Show the property level defaults.
assert(feature.anObject.aString == "yes")
assert(feature.anObjectWithNewDefaults.aString == "YES: overridden from the CONSTRUCTOR!")
assert(feature.anObjectWithFeatureDefaults.aString == "yes")

// It's the same class.
assert(type(of: feature.anObject) == type(of: feature.anObjectWithNewDefaults))
assert(type(of: feature.anObject) == type(of: feature.anObjectWithFeatureDefaults))

assert(feature.anObject.nested.propertySource == "example-object-property-via-constructor")
assert(feature.anObjectWithNewDefaults.nested.propertySource == "an-object-with-new-defaults-constructor")
assert(feature.anObjectWithFeatureDefaults.nested.propertySource == "example-object-property-via-constructor")

// Test if we can override the defaults with JSON coming from Nimbus.
let api = HardcodedNimbusFeatures(with: ["with-objects-feature":  """
{
    "an-object-with-feature-defaults": {
        "a-string": "Sounds good",
        "nested": {
            "property-source": "from-json"
        }
    }
}
"""])
nimbus.api = api
nimbus.invalidateCachedValues()

// Now test the selectively overridden properties of the feature.
let feature1 = nimbus.features.withObjectsFeature.value()

assert(feature1.anObject.aString == "yes")
assert(feature1.anObjectWithFeatureDefaults.aString == "Sounds good")

assert(feature1.anObject.nested.propertySource == "example-object-property-via-constructor")
assert(feature1.anObjectWithNewDefaults.nested.propertySource == "an-object-with-new-defaults-constructor")
assert(feature1.anObjectWithFeatureDefaults.nested.propertySource == "from-json")

// Record the exposure and test it.
nimbus.features.withObjectsFeature.recordExposure()
assert(api.isExposed(featureId: "with-objects-feature"))

// Just to make sure, the `feature` object that we used earlier is still giving the same values, taken
// from the property defaults.
assert(feature.anObject.aString == "yes")
assert(feature.anObjectWithFeatureDefaults.aString == "yes")
