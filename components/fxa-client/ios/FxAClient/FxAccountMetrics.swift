/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation

public struct MetricsParams {
    public let flowId: String?
    public let flowBeginTime: UInt64?
    public let deviceId: String?
    public let utmSource: String?
    public let utmContent: String?
    public let utmMedium: String?
    public let utmTerm: String?
    public let utmCampaign: String?
    public let entrypointExperiment: String?
    public let entrypointVariation: String?

    internal func toMsg() -> MsgTypes_MetricsParams {
        var msg = MsgTypes_MetricsParams()
        var params = [String: String]()

        if flowId != nil {
            params["flow_id"] = flowId
        }
        if flowBeginTime != nil {
            params["flow_begin_time"] = String(describing: flowBeginTime)
        }
        if deviceId != nil {
            params["device_id"] = deviceId
        }
        if utmSource != nil {
            params["utm_source"] = utmSource
        }
        if utmContent != nil {
            params["utm_content"] = utmContent
        }
        if utmMedium != nil {
            params["utm_medium"] = utmMedium
        }
        if utmTerm != nil {
            params["utm_term"] = flowId
        }
        if utmCampaign != nil {
            params["utm_campaign"] = utmCampaign
        }
        if entrypointExperiment != nil {
            params["entrypoint_experiment"] = entrypointExperiment
        }
        if entrypointVariation != nil {
            params["entrypoint_variation"] = entrypointVariation
        }

        msg.parameters = params
        return msg
    }

    public static func newEmpty() -> MetricsParams {
        return MetricsParams(
            flowId: nil,
            flowBeginTime: nil,
            deviceId: nil,
            utmSource: nil,
            utmContent: nil,
            utmMedium: nil,
            utmTerm: nil,
            utmCampaign: nil,
            entrypointExperiment: nil,
            entrypointVariation: nil
        )
    }
}
