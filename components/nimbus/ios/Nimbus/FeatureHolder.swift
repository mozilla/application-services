/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

public typealias GetSdk = () -> FeaturesInterface?

public class FeatureHolder<T> {
    private let getSdk: GetSdk
    private let featureId: String
    private let create: (Variables) -> T

    public init(_ getSdk: @escaping () -> FeaturesInterface?,
                featureId: String,
                with create: @escaping (Variables) -> T)
    {
        self.getSdk = getSdk
        self.featureId = featureId
        self.create = create
    }

    public func value() -> T {
        let variables = getSdk()?.getVariables(featureId: featureId, sendExposureEvent: false) ?? NilVariables.instance
        return create(variables)
    }

    public func recordExposure() {
        getSdk()?.recordExposureEvent(featureId: featureId)
    }
}
