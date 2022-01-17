import FeatureManifest

assert(add(a: 1, b: 2) == 3)
print("hello world")

let mn = MockNimbus(pairs: ("a", "{ \"a\": 12 }"))
if let variables = mn.getVariables(featureId: "a", recordExposureEvent: false) {
    assert(variables.getInt("a") == 12)
    print("Printing the value: " + String(variables.getInt("a")!))
} else {
    fatalError("Variables is not defined")
}
assert(mn.getExposureCount(featureId: "a") == 0)
mn.recordExposureEvent(featureId: "a")
assert(mn.getExposureCount(featureId: "a") == 1)
