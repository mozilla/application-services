/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
import Foundation
import Glean

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

    private var create: (Variables) -> T

    public init(_ getSdk: @escaping () -> FeaturesInterface?,
                featureId: String,
                with create: @escaping (Variables) -> T)
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
        if let sdk = getSdk() {
            variables = sdk.getVariables(featureId: featureId, sendExposureEvent: false) ?? NilVariables.instance
        } else {
            GleanMetrics.NimbusHealth.featureRequestError.record(GleanMetrics.NimbusHealth.FeatureRequestErrorExtra(
                featureId: featureId
            ))
        }
        let v = create(variables)
        cachedValue = v
        return v
    }

    /// Send an exposure event for this feature. This should be done when the user is shown the feature, and may change
    /// their behavior because of it.
    public func recordExposure() {
        getSdk()?.recordExposureEvent(featureId: featureId)
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
    public func with(initializer: @escaping (Variables) -> T) {
        lock.lock()
        defer { self.lock.unlock() }
        cachedValue = nil
        create = initializer
    }
}
