/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.places

import com.sun.jna.Pointer
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject

open class PlacesConnection(path: String, encryption_key: String? = null) : AutoCloseable {

    private var db: RawPlacesConnection? = null

    init {
        this.db = rustCall { error ->
            LibPlacesFFI.INSTANCE.places_connection_new(path, encryption_key, error)
        }
    }

    @Synchronized
    override fun close() {
        val db = this.db
        this.db = null
        if (db != null) {
            LibPlacesFFI.INSTANCE.places_connection_destroy(db)
        }
    }

    fun noteObservation(data: VisitObservation) {
        val json = data.toJSON().toString()
        rustCall { error ->
            LibPlacesFFI.INSTANCE.places_note_observation(this.db!!, json, error)
        }
    }

    fun queryAutocomplete(query: String, limit: Int = 10): List<SearchResult> {
        val json = rustCallForString { error ->
            LibPlacesFFI.INSTANCE.places_query_autocomplete(this.db!!, query, limit, error)
        }
        return SearchResult.fromJSONArray(json)
    }

    fun getVisited(urls: List<String>): List<Boolean> {
        val urlsToJson = JSONArray()
        for (url in urls) {
            urlsToJson.put(url)
        }
        val urlStr = urls.toString()
        val visitedStr = rustCallForString { error ->
            LibPlacesFFI.INSTANCE.places_get_visited(this.db!!, urlStr, error)
        }
        val visited = JSONArray(visitedStr)
        val result = mutableListOf<Boolean>()
        for (index in 0 until visited.length()) {
            result.add(visited.getBoolean(index))
        }
        return result
    }

    /** NB: start and end are unix timestamps in milliseconds! */
    fun getVisitedUrlsInRange(start: Long, end: Long = Long.MAX_VALUE, includeRemote: Boolean = true): List<String> {
        val urlsJson = rustCallForString { error ->
            val incRemoteArg: Byte = if (includeRemote) { 1 } else { 0 }
            LibPlacesFFI.INSTANCE.places_get_visited_urls_in_range(
                    this.db!!, start, end, incRemoteArg, error)
        }
        val arr = JSONArray(urlsJson)
        val result = mutableListOf<String>();
        for (idx in 0 until arr.length()) {
            result.add(arr.getString(idx))
        }
        return result
    }

    private inline fun <U> rustCall(callback: (RustError.ByReference) -> U): U {
        synchronized(this) {
            val e = RustError.ByReference()
            val ret: U = callback(e)
            if (e.isFailure()) {
                throw e.intoException()
            } else {
                return ret
            }
        }
    }

    private inline fun rustCallForString(callback: (RustError.ByReference) -> Pointer?): String {
        val cstring = rustCall(callback)
                ?: throw RuntimeException("Bug: Don't use this function when you can return" +
                                          " null on success.")
        try {
            return cstring.getString(0, "utf8")
        } finally {
            LibPlacesFFI.INSTANCE.places_destroy_string(cstring)
        }
    }

    companion object {
        // Constants for use on VisitObservation visitType
        /** This transition type means the user followed a link. */
        const val VISIT_TYPE_LINK: Int = 1
        /** This transition type means that the user typed the page's URL in the
         *  URL bar or selected it from UI (URL bar autocomplete results, etc).
         */
        const val VISIT_TYPE_TYPED: Int = 2
        // TODO: rest of docs
        const val VISIT_TYPE_BOOKMARK = 3
        const val VISIT_TYPE_EMBED = 4
        const val VISIT_TYPE_REDIRECT_PERMANENT = 5
        const val VISIT_TYPE_REDIRECT_TEMPORARY = 6
        const val VISIT_TYPE_DOWNLOAD = 7
        const val VISIT_TYPE_FRAMED_LINK = 8
        const val VISIT_TYPE_RELOAD = 9
    }
}

open class PlacesException(msg: String): Exception(msg)
open class InternalPanic(msg: String): PlacesException(msg)
open class UrlParseFailed(msg: String): PlacesException(msg)
open class InvalidPlaceInfo(msg: String): PlacesException(msg)

data class VisitObservation(
    val url: String,
    val visitType: Int? = null,
    val title: String? = null,
    val isError: Boolean? = null,
    val isRedirectSource: Boolean? = null,
    val isPermanentRedirectSource: Boolean? = null,
    /** Milliseconds */
    val at: Long? = null,
    val referrer: String? = null,
    val isRemote: Boolean? = null
) {
    fun toJSON(): JSONObject {
        val o = JSONObject()
        o.put("url", this.url)
        this.visitType?.let { o.put("visit_type", it) }
        this.title?.let { o.put("title", it) }
        this.isError?.let { o.put("is_error", it) }
        this.isRedirectSource?.let { o.put("is_redirect_source", it) }
        this.isPermanentRedirectSource?.let { o.put("is_permanent_redirect_source", it) }
        this.at?.let { o.put("at", it) }
        this.referrer?.let { o.put("referrer", it) }
        this.isRemote?.let { o.put("is_remote", it) }
        return o
    }
}

data class SearchResult(
    val searchString: String,
    val url: String,
    val title: String,
    val frecency: Long,
    val iconUrl: String? = null
    // Skipping `reasons` for now...
) {
    companion object {
        fun fromJSON(jsonObject: JSONObject): SearchResult {
            fun stringOrNull(key: String): String? {
                return try {
                    jsonObject.getString(key)
                } catch (e: JSONException) {
                    null
                }
            }

            return SearchResult(
                searchString = jsonObject.getString("search_string"),
                url = jsonObject.getString("url"),
                title = jsonObject.getString("title"),
                frecency = jsonObject.getLong("frecency"),
                iconUrl = stringOrNull("icon_url")
            )
        }

        fun fromJSONArray(jsonArrayText: String): List<SearchResult> {
            val result: MutableList<SearchResult> = mutableListOf()
            val array = JSONArray(jsonArrayText)
            for (index in 0 until array.length()) {
                result.add(fromJSON(array.getJSONObject(index)))
            }
            return result
        }
    }
}
