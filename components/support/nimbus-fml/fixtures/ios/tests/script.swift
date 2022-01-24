import FeatureManifest

assert(add(a: 1, b: 2) == 3)

let mn = MockNimbus(("a", "{ \"a\": 12 }"))
let variables = mn.getVariables(featureId: "a", sendExposureEvent: false)
assert(variables.getInt("a") == 12)

assert(mn.getExposureCount(featureId: "a") == 0)
mn.recordExposureEvent(featureId: "a")
assert(mn.getExposureCount(featureId: "a") == 1)
