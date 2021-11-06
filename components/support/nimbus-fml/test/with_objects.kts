/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import org.mozilla.experiments.nimbus.MockNimbus

// Get the feature from the MyNimbus.features.
// The api isn't ready yet.
val feature = MyNimbus.features.withObjectsFeature.value()

// Show the property level defaults.
assert(feature.anObject.aString == "yes")
assert(feature.anObjectWithFeatureDefaults.aString == "yes")
assert(feature.anObject.javaClass == feature.anObjectWithFeatureDefaults.javaClass) // It's the same class.

// Test if we can override the defaults with JSON coming from Nimbus.
val api = MockNimbus("with-objects-feature" to """{
    "an-object-with-feature-defaults": {
        "a-string": "Sounds good"
    }
}""")
MyNimbus.api = api

// Side test: we just configured a feature with the defaults shipped with the app: the MyNimbus.api wasn't
// set when we needed the feature, so the defaults were used. Likely we shouldn't count
// that as an exposure.
MyNimbus.features.withObjectsFeature.recordExposure()
assert(!api.isExposed("with-objects-feature"))

// Now test the selectively overidden properties of the feature.
val feature1 = MyNimbus.features.withObjectsFeature.value()

assert(feature1.anObject.aString == "yes")
assert(feature1.anObjectWithFeatureDefaults.aString == "Sounds good")

// Record the exposure and test it.
MyNimbus.features.withObjectsFeature.recordExposure()
assert(api.isExposed("with-objects-feature"))

// Just to make sure, the `feature` object that we used earlier is still giving the same values, taken
// from the property defaults.
assert(feature.anObject.aString == "yes")
assert(feature.anObjectWithFeatureDefaults.aString == "yes")
