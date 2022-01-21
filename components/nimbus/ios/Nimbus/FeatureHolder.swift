
public class FeatureHolder<T> {
    private let apiFn: () -> FeaturesInterface?
    private let featureId: String
    private let create: (Variables?) -> T
    private var exposureRecorder: (() -> ())? = nil
    
    public init(_ apiFn: @escaping () -> FeaturesInterface?, _ featureId: String, _ create: @escaping (Variables?) -> T) {
        self.apiFn = apiFn
        self.featureId = featureId
        self.create = create
    }

    public func value() -> T {
        let api = self.apiFn()
        let feature = self.create(api?.getVariables(featureId: featureId, recordExposureEvent: false))
        if let api = api {
            weak var weakApi = api
            self.exposureRecorder = { () in
                weakApi?.recordExposureEvent(featureId: self.featureId)
            }
        }
        return feature
    }

    public func recordExposure() {
        self.exposureRecorder?()
    }
}
