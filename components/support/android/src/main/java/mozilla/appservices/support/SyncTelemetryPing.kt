/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.support

import org.json.JSONArray
import org.json.JSONObject

enum class FailureName {
    Shutdown,
    Other,
    Unexpected,
    Auth,
    Http
}

data class SyncTelemetryPing(
    val version: Int,
    val uid: String?,
    val events: List<EventInfo>,
    val syncs: List<SyncInfo>
) {
    companion object {
        fun fromJSON(jsonObject: JSONObject): SyncTelemetryPing {
            val events = unwrapFromJSON(jsonObject) {
                it.getJSONArray("events")
            }?.let {
                EventInfo.fromJSONArray(it)
            } ?: emptyList()
            val syncs = unwrapFromJSON(jsonObject) {
                it.getJSONArray("syncs")
            }?.let {
                SyncInfo.fromJSONArray(it)
            } ?: emptyList()
            return SyncTelemetryPing(
                version = jsonObject.getInt("version"),
                uid = stringOrNull(jsonObject, "uid"),
                events = events,
                syncs = syncs
            )
        }

        fun fromJSONString(jsonObjectText: String): SyncTelemetryPing {
            return fromJSON(JSONObject(jsonObjectText))
        }
    }
}

data class SyncInfo(
    val at: Int,
    val took: Int,
    val engines: List<EngineInfo>,
    val failureReason: FailureReason?
) {
    companion object {
        fun fromJSON(jsonObject: JSONObject): SyncInfo {
            val failureReason = jsonObject.getJSONObject("failureReason")?.let {
                FailureReason.fromJSON(it)
            }
            val engines = unwrapFromJSON(jsonObject) {
                it.getJSONArray("engines")
            }?.let {
                EngineInfo.fromJSONArray(it)
            } ?: emptyList()
            return SyncInfo(
                at = jsonObject.getInt("when"),
                took = intOrZero(jsonObject, "took"),
                engines = engines,
                failureReason = failureReason
            )
        }

        fun fromJSONArray(jsonArray: JSONArray): List<SyncInfo> {
            val result: MutableList<SyncInfo> = mutableListOf()
            for (index in 0 until jsonArray.length()) {
                result.add(fromJSON(jsonArray.getJSONObject(index)))
            }
            return result
        }
    }
}

data class EngineInfo(
    val name: String,
    val at: Int,
    val took: Int,
    val incoming: IncomingInfo?,
    val outgoing: List<OutgoingInfo>,
    val failureReason: FailureReason?
) {
    companion object {
        fun fromJSON(jsonObject: JSONObject): EngineInfo {
            val incoming = unwrapFromJSON(jsonObject) {
                it.getJSONObject("incoming")
            }?.let {
                IncomingInfo.fromJSON(it)
            }
            val outgoing = unwrapFromJSON(jsonObject) {
                it.getJSONArray("outgoing")
            }?.let {
                OutgoingInfo.fromJSONArray(it)
            } ?: emptyList()
            val failureReason = unwrapFromJSON(jsonObject) {
                jsonObject.getJSONObject("failureReason")
            }?.let {
                FailureReason.fromJSON(it)
            }
            return EngineInfo(
                name = jsonObject.getString("name"),
                at = jsonObject.getInt("when"),
                took = intOrZero(jsonObject, "took"),
                incoming = incoming,
                outgoing = outgoing,
                failureReason = failureReason
            )
        }

        fun fromJSONArray(jsonArray: JSONArray): List<EngineInfo> {
            val result: MutableList<EngineInfo> = mutableListOf()
            for (index in 0 until jsonArray.length()) {
                result.add(fromJSON(jsonArray.getJSONObject(index)))
            }
            return result
        }
    }
}

data class IncomingInfo(
    val applied: Int,
    val failed: Int,
    val newFailed: Int,
    val reconciled: Int
) {
    companion object {
        fun fromJSON(jsonObject: JSONObject): IncomingInfo {
            return IncomingInfo(
                applied = intOrZero(jsonObject, "applied"),
                failed = intOrZero(jsonObject, "failed"),
                newFailed = intOrZero(jsonObject, "newFailed"),
                reconciled = intOrZero(jsonObject, "reconciled")
            )
        }
    }
}

data class OutgoingInfo(
    val sent: Int,
    val failed: Int
) {
    companion object {
        fun fromJSON(jsonObject: JSONObject): OutgoingInfo {
            return OutgoingInfo(
                sent = intOrZero(jsonObject, "sent"),
                failed = intOrZero(jsonObject, "failed")
            )
        }

        fun fromJSONArray(jsonArray: JSONArray): List<OutgoingInfo> {
            val result: MutableList<OutgoingInfo> = mutableListOf()
            for (index in 0 until jsonArray.length()) {
                result.add(fromJSON(jsonArray.getJSONObject(index)))
            }
            return result
        }
    }
}

data class FailureReason (
    val name: FailureName,
    val message: String?,
    val code: Int
) {
    companion object {
        fun fromJSON(jsonObject: JSONObject): FailureReason? {
            return jsonObject.getString("name")?.let {
                when (it) {
                    "shutdownerror" -> FailureReason(
                        name = FailureName.Shutdown,
                        message = null,
                        code = -1
                    )
                    "othererror" -> FailureReason(
                        name = FailureName.Other,
                        message = jsonObject.getString("error"),
                        code = -1
                    )
                    "autherror" -> FailureReason(
                        name = FailureName.Auth,
                        message = jsonObject.getString("from"),
                        code = -1
                    )
                    "httperror" -> FailureReason(
                        name = FailureName.Http,
                        message = null,
                        code = jsonObject.getInt("code")
                    )
                    else -> FailureReason(
                        name = FailureName.Unexpected,
                        message = jsonObject.getString("error"),
                        code = -1
                    )
                }
            }
        }
    }
}

data class EventInfo(
    val obj: String,
    val method: String,
    val value: String?,
    val extra: Map<String, String>
) {
    companion object {
        fun fromJSON(jsonObject: JSONObject): EventInfo {
            val extra = unwrapFromJSON(jsonObject) {
                jsonObject.getJSONObject("extra")
            }?.let {
                val extra = mutableMapOf<String, String>()
                for (key in it.keys()) {
                    extra[key] = it.getString(key)
                }
                extra
            } ?: emptyMap<String, String>()
            return EventInfo(
                obj = jsonObject.getString("object"),
                method = jsonObject.getString("method"),
                value = stringOrNull(jsonObject, "value"),
                extra = extra
            )
        }

        fun fromJSONArray(jsonArray: JSONArray): List<EventInfo> {
            val result: MutableList<EventInfo> = mutableListOf()
            for (index in 0 until jsonArray.length()) {
                result.add(fromJSON(jsonArray.getJSONObject(index)))
            }
            return result
        }
    }
}

private fun intOrZero(jsonObject: JSONObject, key: String): Int {
    return unwrapFromJSON(jsonObject) {
        it.getInt(key)
    } ?: 0
}
