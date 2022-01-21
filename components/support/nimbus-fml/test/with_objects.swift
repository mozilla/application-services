// Get the feature from the MyNimbus.features.
// The api isn't ready yet.
import FeatureManifest
import Foundation

let feature = MyNimbus.features.withObjectsFeature.value()

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
let api = MockNimbus(("with-objects-feature",  """
{
    "an-object-with-feature-defaults": {
        "a-string": "Sounds good",
        "nested": {
            "property-source": "from-json"
        }
    }
}
"""))
MyNimbus.api = api

// Side test: we just configured a feature with the defaults shipped with the app: the MyNimbus.api wasn't
// set when we needed the feature, so the defaults were used. Likely we shouldn't count
// that as an exposure.
MyNimbus.features.withObjectsFeature.recordExposure()
assert(!api.isExposed(featureId: "with-objects-feature"))

// Now test the selectively overidden properties of the feature.
let feature1 = MyNimbus.features.withObjectsFeature.value()

assert(feature1.anObject.aString == "yes")
assert(feature1.anObjectWithFeatureDefaults.aString == "Sounds good")

assert(feature1.anObject.nested.propertySource == "example-object-property-via-constructor")
assert(feature1.anObjectWithNewDefaults.nested.propertySource == "an-object-with-new-defaults-constructor")
assert(feature1.anObjectWithFeatureDefaults.nested.propertySource == "from-json")

// Record the exposure and test it.
MyNimbus.features.withObjectsFeature.recordExposure()
assert(api.isExposed(featureId: "with-objects-feature"))

// Just to make sure, the `feature` object that we used earlier is still giving the same values, taken
// from the property defaults.
assert(feature.anObject.aString == "yes")
assert(feature.anObjectWithFeatureDefaults.aString == "yes")
