/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
import Foundation

public typealias GetSdk = () -> FeaturesInterface?

/// `FeatureHolder` is a class that unpacks a JSON object from the Nimbus SDK and transforms it into a useful
/// type safe object, generated from a feature manifest (a `.fml.yaml` file).
///
/// The two routinely useful methods are the `value()` and `recordExposure()` events.
///
/// There are methods useful for testing, and more advanced uses: these all start with `with`.
///
public class FeatureHolder<T> {
    private let lock = NSLock()
    private var cachedValue: T?

    private var getSdk: GetSdk
    private let featureId: String

    private var create: (Variables, UserDefaults?) -> T

    public init(_ getSdk: @escaping () -> FeaturesInterface?,
                featureId: String,
                with create: @escaping (Variables, UserDefaults?) -> T)
    {
        self.getSdk = getSdk
        self.featureId = featureId
        self.create = create
    }

    /// Get the JSON configuration from the Nimbus SDK and transform it into a configuration object as specified
    /// in the feature manifest. This is done each call of the method, so the method should be called once, and the
    /// result used for the configuration of the feature.
    ///
    /// Some care is taken to cache the value, this is for performance critical uses of the API.
    /// It is possible to invalidate the cache with `FxNimbus.invalidateCachedValues()` or `with(cachedValue: nil)`.
    public func value() -> T {
        lock.lock()
        defer { self.lock.unlock() }
        if let v = cachedValue {
            return v
        }
        var variables: Variables = NilVariables.instance
        var defaults: UserDefaults?
        if let sdk = getSdk() {
            variables = sdk.getVariables(featureId: featureId, sendExposureEvent: false)
            defaults = sdk.userDefaults
        }
        let v = create(variables, defaults)
        cachedValue = v
        return v
    }

    /// Send an exposure event for this feature. This should be done when the user is shown the feature, and may change
    /// their behavior because of it.
    public func recordExposure() {
        getSdk()?.recordExposureEvent(featureId: featureId, experimentSlug: nil)
    }

    /// Send an exposure event for this feature, in the given experiment.
    ///
    /// If the experiment does not exist, or the client is not enrolled in that experiment, then no exposure event
    /// is recorded.
    ///
    /// If you are not sure of the experiment slug, then this is _not_ the API you need: you should use
    /// {recordExposure} instead.
    ///
    /// - Parameter slug the experiment identifier, likely derived from the {value}.
    public func recordExperimentExposure(slug: String) {
        getSdk()?.recordExposureEvent(featureId: featureId, experimentSlug: slug)
    }

    /// Send a malformed feature event for this feature.
    ///
    /// - Parameter partId an optional detail or part identifier to be attached to the event.
    public func recordMalformedConfiguration(with partId: String = "") {
        getSdk()?.recordMalformedConfiguration(featureId: featureId, with: partId)
    }

    /// Is this feature the focus of an automated test.
    ///
    /// A utility flag to be used in conjunction with {HardcodedNimbusFeatures}.
    ///
    /// It is intended for use for app-code to detect when the app is under test, and
    /// take steps to make itself easier to test.
    ///
    /// These cases should be rare, and developers should look for other ways to test
    /// code without relying on this facility.
    ///
    /// For example, a background worker might be scheduled to run every 24 hours, but
    /// under test it would be desirable to run immediately, and only once.
    public func isUnderTest() -> Bool {
        lock.lock()
        defer { self.lock.unlock() }

        guard let features = getSdk() as? HardcodedNimbusFeatures else {
            return false
        }
        return features.has(featureId: featureId)
    }

    /// This overwrites the cached value with the passed one.
    ///
    /// This is most likely useful during testing only.
    public func with(cachedValue value: T?) {
        lock.lock()
        defer { self.lock.unlock() }
        cachedValue = value
    }

    /// This resets the SDK and clears the cached value.
    ///
    /// This is especially useful at start up and for imported features.
    public func with(sdk: @escaping () -> FeaturesInterface?) {
        lock.lock()
        defer { self.lock.unlock() }
        getSdk = sdk
        cachedValue = nil
    }

    /// This changes the mapping between a `Variables` and the feature configuration object.
    ///
    /// This is most likely useful during testing and other generated code.
    public func with(initializer: @escaping (Variables, UserDefaults?) -> T) {
        lock.lock()
        defer { self.lock.unlock() }
        cachedValue = nil
        create = initializer
    }
}
