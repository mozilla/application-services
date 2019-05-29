/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.sync15

import mozilla.appservices.support.stringOrNull
import mozilla.appservices.support.unwrapFromJSON
import org.json.JSONArray
import org.json.JSONObject

enum class FailureName {
    Shutdown,
    Other,
    Unexpected,
    Auth,
    Http,
    Unknown
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

    fun toJSON(): JSONObject {
        return JSONObject().apply {
            put("version", version)
            uid?.let {
                put("uid", it)
            }
            if (!events.isEmpty()) {
                val jsonArray = JSONArray().apply {
                    events.forEach {
                        put(it.toJSON())
                    }
                }
                put("events", jsonArray)
            }
            if (!syncs.isEmpty()) {
                val jsonArray = JSONArray().apply {
                    syncs.forEach {
                        put(it.toJSON())
                    }
                }
                put("syncs", jsonArray)
            }
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
            val engines = unwrapFromJSON(jsonObject) {
                it.getJSONArray("engines")
            }?.let {
                EngineInfo.fromJSONArray(it)
            } ?: emptyList()
            val failureReason = unwrapFromJSON(jsonObject) {
                it.getJSONObject("failureReason")
            }?.let {
                FailureReason.fromJSON(it)
            }
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

    fun toJSON(): JSONObject {
        return JSONObject().apply {
            put("when", at)
            if (took > 0) {
                put("took", took)
            }
            if (!engines.isEmpty()) {
                val jsonArray = JSONArray().apply {
                    engines.forEach {
                        put(it.toJSON())
                    }
                }
                put("engines", jsonArray)
            }
            failureReason?.let {
                put("failureReason", it.toJSON())
            }
        }
    }
}

data class EngineInfo(
    val name: String,
    val at: Int,
    val took: Int,
    val incoming: IncomingInfo?,
    val outgoing: List<OutgoingInfo>,
    val failureReason: FailureReason?,
    val validation: ValidationInfo?
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
            val validation = unwrapFromJSON(jsonObject) {
                jsonObject.getJSONObject("validation")
            }?.let {
                ValidationInfo.fromJSON(it)
            }
            return EngineInfo(
                name = jsonObject.getString("name"),
                at = jsonObject.getInt("when"),
                took = intOrZero(jsonObject, "took"),
                incoming = incoming,
                outgoing = outgoing,
                failureReason = failureReason,
                validation = validation
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

    fun toJSON(): JSONObject {
        return JSONObject().apply {
            put("name", name)
            put("when", at)
            if (took > 0) {
                put("took", took)
            }
            incoming?.let {
                put("incoming", it.toJSON())
            }
            if (!outgoing.isEmpty()) {
                val jsonArray = JSONArray().apply {
                    outgoing.forEach {
                        put(it.toJSON())
                    }
                }
                put("outgoing", jsonArray)
            }
            failureReason?.let {
                put("failureReason", it.toJSON())
            }
            validation?.let {
                put("validation", it.toJSON())
            }
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

    fun toJSON(): JSONObject {
        return JSONObject().apply {
            if (applied > 0) {
                put("applied", applied)
            }
            if (failed > 0) {
                put("failed", failed)
            }
            if (newFailed > 0) {
                put("newFailed", newFailed)
            }
            if (reconciled > 0) {
                put("reconciled", reconciled)
            }
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

    fun toJSON(): JSONObject {
        return JSONObject().apply {
            if (sent > 0) {
                put("sent", sent)
            }
            if (failed > 0) {
                put("failed", failed)
            }
        }
    }
}

data class ValidationInfo(
    val version: Int,
    val problems: List<ProblemInfo>,
    val failureReason: FailureReason?
) {
    companion object {
        fun fromJSON(jsonObject: JSONObject): ValidationInfo {
            val problems = unwrapFromJSON(jsonObject) {
                it.getJSONArray("outgoing")
            }?.let {
                ProblemInfo.fromJSONArray(it)
            } ?: emptyList()
            val failureReason = unwrapFromJSON(jsonObject) {
                it.getJSONObject("failureReason")
            }?.let {
                FailureReason.fromJSON(it)
            }
            return ValidationInfo(
                version = jsonObject.getInt("version"),
                problems = problems,
                failureReason = failureReason
            )
        }
    }

    fun toJSON(): JSONObject {
        return JSONObject().apply {
            put("version", version)
            if (!problems.isEmpty()) {
                val jsonArray = JSONArray().apply {
                    problems.forEach {
                        put(it.toJSON())
                    }
                }
                put("problems", jsonArray)
            }
            failureReason?.let {
                put("failueReason", it.toJSON())
            }
        }
    }
}

data class ProblemInfo(
    val name: String,
    val count: Int
) {
    companion object {
        fun fromJSON(jsonObject: JSONObject): ProblemInfo {
            return ProblemInfo(
                name = jsonObject.getString("name"),
                count = intOrZero(jsonObject, "count")
            )
        }

        fun fromJSONArray(jsonArray: JSONArray): List<ProblemInfo> {
            val result: MutableList<ProblemInfo> = mutableListOf()
            for (index in 0 until jsonArray.length()) {
                result.add(fromJSON(jsonArray.getJSONObject(index)))
            }
            return result
        }
    }

    fun toJSON(): JSONObject {
        return JSONObject().apply {
            put("name", name)
            if (count > 0) {
                put("count", count)
            }
        }
    }
}

data class FailureReason (
    val name: FailureName,
    val message: String? = null,
    val code: Int = -1
) {
    companion object {
        fun fromJSON(jsonObject: JSONObject): FailureReason? {
            return jsonObject.getString("name")?.let {
                when (it) {
                    "shutdownerror" -> FailureReason(
                        name = FailureName.Shutdown
                    )
                    "othererror" -> FailureReason(
                        name = FailureName.Other,
                        message = jsonObject.getString("error")
                    )
                    "unexpectederror" -> FailureReason(
                        name = FailureName.Unexpected,
                        message = jsonObject.getString("error")
                    )
                    "autherror" -> FailureReason(
                        name = FailureName.Auth,
                        message = jsonObject.getString("from")
                    )
                    "httperror" -> FailureReason(
                        name = FailureName.Http,
                        code = jsonObject.getInt("code")
                    )
                    else -> FailureReason(
                        name = FailureName.Unknown
                    )
                }
            }
        }
    }

    fun toJSON(): JSONObject {
        return JSONObject().apply {
            when (name) {
                FailureName.Shutdown -> {
                    put("name", "shutdownerror")
                }
                FailureName.Other -> {
                    put("name", "othererror")
                    message?.let {
                        put("error", it)
                    }
                }
                FailureName.Unexpected, FailureName.Unknown -> {
                    put("name", "unexpectederror")
                    message?.let {
                        put("error", it)
                    }
                }
                FailureName.Auth -> {
                    put("name", "autherror")
                    message?.let {
                        put("from", it)
                    }
                }
                FailureName.Http -> {
                    put("name", "httperror")
                    put("code", code)
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

    fun toJSON(): JSONObject {
        return JSONObject().apply {
            put("object", obj)
            put("method", method)
            value?.let {
                put("value", it)
            }
            if (!extra.isEmpty()) {
                put("extra", extra)
            }
        }
    }
}

private fun intOrZero(jsonObject: JSONObject, key: String): Int {
    return unwrapFromJSON(jsonObject) {
        it.getInt(key)
    } ?: 0
}
