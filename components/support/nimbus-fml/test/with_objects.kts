/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import org.mozilla.experiments.nimbus.MockNimbus
import android.content.Context as MockContext

// Get the feature from the MyNimbus.features.
// The api isn't ready yet.
val ctx = MockContext()
var injected: MockNimbus? = null
MyNimbus.initialize { injected }
val feature = MyNimbus.features.withObjectsFeature.value()

// Show the property level defaults.
assert(feature.anObject.aString == "yes")
assert(feature.anObjectWithNewDefaults.aString == "YES: overridden from the CONSTRUCTOR!")
assert(feature.anObjectWithFeatureDefaults.aString == "yes")

// It's the same class.
assert(feature.anObject.javaClass == feature.anObjectWithNewDefaults.javaClass)
assert(feature.anObject.javaClass == feature.anObjectWithFeatureDefaults.javaClass)

assert(feature.anObject.nested.propertySource == "example-object-property-via-constructor")
assert(feature.anObjectWithNewDefaults.nested.propertySource == "an-object-with-new-defaults-constructor")
assert(feature.anObjectWithFeatureDefaults.nested.propertySource == "example-object-property-via-constructor")

// Test if we can override the defaults with JSON coming from Nimbus.
val api = MockNimbus("with-objects-feature" to """{
    "an-object-with-feature-defaults": {
        "a-string": "Sounds good",
        "nested": {
            "property-source": "from-json"
        }
    }
}""")
injected = api
MyNimbus.invalidateCachedValues()

// Now test the selectively overridden properties of the feature.
val feature1 = MyNimbus.features.withObjectsFeature.value()

assert(feature1.anObject.aString == "yes")
assert(feature1.anObjectWithFeatureDefaults.aString == "Sounds good")

assert(feature1.anObject.nested.propertySource == "example-object-property-via-constructor")
assert(feature1.anObjectWithNewDefaults.nested.propertySource == "an-object-with-new-defaults-constructor")
assert(feature1.anObjectWithFeatureDefaults.nested.propertySource == "from-json")

// Record the exposure and test it.
MyNimbus.features.withObjectsFeature.recordExposure()
assert(api.isExposed("with-objects-feature"))

// Just to make sure, the `feature` object that we used earlier is still giving the same values, taken
// from the property defaults.
assert(feature.anObject.aString == "yes")
assert(feature.anObjectWithFeatureDefaults.aString == "yes")
