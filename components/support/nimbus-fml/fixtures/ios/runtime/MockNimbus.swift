/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */
import Foundation

public class MockNimbus: FeaturesInterface {
    var map: [String: Any] = [:]
    public init(_ pairs: (String, String)...) {
        for (key, value) in pairs {
            if let data = value.data(using: .utf8) {
                do {
                    let json = try JSONSerialization.jsonObject(with: data, options: .mutableContainers)
                    map[key] = json
                } catch {
                    fatalError("Invalid JSON Passed")
                }
            }
        }
    }

    public func getVariables(featureId: String, recordExposureEvent: Bool = true) -> Variables? {
        if let json = map[featureId] {
            return JSONVariables(with: json as! [String: Any])
        }
        return nil
    }

    private var exposureCounts: [String: Int] = [:]

    public func recordExposureEvent(featureId: String) {
        if map[featureId] != nil {
            exposureCounts[featureId] = getExposureCount(featureId: featureId) + 1
        }
    }

    public func getExposureCount(featureId: String) -> Int {
        if let count = exposureCounts[featureId] {
            return count
        }
        return 0
    }

    public func isExposed(featureId: String) -> Bool {
        getExposureCount(featureId: featureId) > 0
    }
}
