/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

public class FeatureHolder<T> {
    private let apiFn: () -> FeaturesInterface?
    private let featureId: String
    private let create: (Variables) -> T
    private var exposureRecorder: (() -> Void)?

    public init(_ apiFn: @escaping () -> FeaturesInterface?,
                _ featureId: String,
                _ create: @escaping (Variables) -> T)
    {
        self.apiFn = apiFn
        self.featureId = featureId
        self.create = create
    }

    public func value() -> T {
        let api = apiFn()
        let feature = create(api?.getVariables(featureId: featureId, sendExposureEvent: false) ?? NilVariables.instance)
        if let api = api {
            weak var weakApi = api
            exposureRecorder = { () in
                weakApi?.recordExposureEvent(featureId: self.featureId)
            }
        }
        return feature
    }

    public func recordExposure() {
        exposureRecorder?()
    }
}
