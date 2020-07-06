/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.fxaclient

data class MetricsParams(
    val flowId: String? = null,
    val flowBeginTime: Long? = null,
    val deviceId: String? = null,
    val utmSource: String? = null,
    val utmContent: String? = null,
    val utmMedium: String? = null,
    val utmTerm: String? = null,
    val utmCampaign: String? = null,
    val entrypointExperiment: String? = null,
    val entrypointVariation: String? = null
) {
    fun intoMessage(): MsgTypes.MetricsParams {
        var metricsParamsBuilder = MsgTypes.MetricsParams.newBuilder()
        if (flowId != null) {
            metricsParamsBuilder = metricsParamsBuilder.putParameters("flow_id", flowId)
        }
        if (flowBeginTime != null) {
            metricsParamsBuilder = metricsParamsBuilder.putParameters("flow_begin_time", flowBeginTime.toString())
        }
        if (deviceId != null) {
            metricsParamsBuilder = metricsParamsBuilder.putParameters("device_id", deviceId)
        }
        if (utmSource != null) {
            metricsParamsBuilder = metricsParamsBuilder.putParameters("utm_source", utmSource)
        }
        if (utmContent != null) {
            metricsParamsBuilder = metricsParamsBuilder.putParameters("utm_content", utmContent)
        }
        if (utmMedium != null) {
            metricsParamsBuilder = metricsParamsBuilder.putParameters("utm_medium", utmMedium)
        }
        if (utmTerm != null) {
            metricsParamsBuilder = metricsParamsBuilder.putParameters("utm_term", utmTerm)
        }
        if (utmCampaign != null) {
            metricsParamsBuilder = metricsParamsBuilder.putParameters("utm_campaign", utmCampaign)
        }
        if (entrypointExperiment != null) {
            metricsParamsBuilder = metricsParamsBuilder.putParameters("entrypoint_experiment", entrypointExperiment)
        }
        if (entrypointVariation != null) {
            metricsParamsBuilder = metricsParamsBuilder.putParameters("entrypoint_variation", entrypointVariation)
        }

        return metricsParamsBuilder.build()
    }
}
