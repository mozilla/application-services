/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.places

import com.sun.jna.Pointer
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject

/**
 * An API for interacting with a Places database.
 *
 * @param path an absolute path to a file that will be used for the internal database.
 * @param encryption_key an optional key used for encrypting/decrypting data stored in the internal
 *  database. If omitted, data will be stored in plaintext.
 */
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

    /**
     * Record a visit to a URL, or update meta information about page URL. See [VisitObservation].
     */
    fun noteObservation(data: VisitObservation) {
        val json = data.toJSON().toString()
        rustCall { error ->
            LibPlacesFFI.INSTANCE.places_note_observation(this.db!!, json, error)
        }
    }

    /**
     * A way to search the internal database tailored for autocompletion purposes.
     *
     * @param query a string to match results against.
     * @param limit a maximum number of results to retrieve.
     * @return a list of [SearchResult] matching the [query], in arbitrary order.
     */
    fun queryAutocomplete(query: String, limit: Int = 10): List<SearchResult> {
        val json = rustCallForString { error ->
            LibPlacesFFI.INSTANCE.places_query_autocomplete(this.db!!, query, limit, error)
        }
        return SearchResult.fromJSONArray(json)
    }

    /**
     * Maps a list of page URLs to a list of booleans indicating if each URL was visited.
     * @param urls a list of page URLs about which "visited" information is being requested.
     * @return a list of booleans indicating visited status of each
     * corresponding page URI from [urls].
     */
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

    /**
     * Returns a list of visited URLs for a given time range.
     *
     * @param start beginning of the range, unix timestamp in milliseconds.
     * @param end end of the range, unix timestamp in milliseconds.
     * @param includeRemote boolean flag indicating whether or not to include remote visits. A visit
     *  is (roughly) considered remote if it didn't originate on the current device.
     */
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
}

open class PlacesException(msg: String): Exception(msg)
open class InternalPanic(msg: String): PlacesException(msg)
open class UrlParseFailed(msg: String): PlacesException(msg)
open class InvalidPlaceInfo(msg: String): PlacesException(msg)

@SuppressWarnings("MagicNumber")
enum class VisitType(val type: Int) {
    /** This isn't a visit, but a request to update meta data about a page */
    UPDATE_PLACE(-1),
    /** This transition type means the user followed a link. */
    LINK(1),
    /** This transition type means that the user typed the page's URL in the
     *  URL bar or selected it from UI (URL bar autocomplete results, etc).
     */
    TYPED(2),
    // TODO: rest of docs
    BOOKMARK(3),
    EMBED(4),
    REDIRECT_PERMANENT(5),
    REDIRECT_TEMPORARY(6),
    DOWNLOAD(7),
    FRAMED_LINK(8),
    RELOAD(9)
}

/**
 * Encapsulates either information about a visit to a page, or meta information about the page,
 * or both. Use [VisitType.UPDATE_PLACE] to differentiate an update from a visit.
 */
data class VisitObservation(
    val url: String,
    val visitType: VisitType,
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
        // Absence of visit_type indicates that this is an update.
        if (this.visitType != VisitType.UPDATE_PLACE) {
            o.put("visit_type", this.visitType.type)
        }
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
