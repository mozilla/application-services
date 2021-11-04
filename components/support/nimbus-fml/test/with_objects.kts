/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import org.mozilla.experiments.nimbus.MockNimbus

// Get the feature from the MyNimbus.features.
// The api isn't ready yet.
val feature = MyNimbus.features.withObjectsFeature.value()

assert(feature.anObject.aString == "yes")

MyNimbus.features.withObjectsFeature.recordExposure()

// Let's give the generated code a Nimbus object with some
// configuration in it to override the defaults.
val api = MockNimbus("with-objects-feature" to """{
    "an-object": {
        "a-string": "YES"
    }
}""")

MyNimbus.api = api
// As an aside, we want to record the exposure on the feature we've just configured.
MyNimbus.features.withObjectsFeature.recordExposure()
// The api won't record the exposre, because it wasn't the one that the feature was
// configured with.
assert(!api.isExposed("with-objects-feature"))

// So now we can compare the feature config from when Nimbus wasn't ready or configured
// to the one where we're overriding parts of the feature with the JSON from the server.
val oldFeature = feature
val newFeature = MyNimbus.features.withObjectsFeature.value()

assert(oldFeature.anObject.aString == "yes")
assert(newFeature.anObject.aString == "YES")

// Let's check that the exposure is recorded properly.
MyNimbus.features.withObjectsFeature.recordExposure()
assert(api.isExposed("with-objects-feature"))
assert(api.getExposureCount("with-objects-feature") == 1)