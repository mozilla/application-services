/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.places

import com.sun.jna.Native
import com.sun.jna.Pointer
import com.sun.jna.StringArray
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.atomic.AtomicLong
import java.util.concurrent.atomic.AtomicReference

/**
 * An implementation of a [PlacesAPI] backed by a Rust Places library.
 *
 * @param path an absolute path to a file that will be used for the internal database.
 * @param encryption_key an optional key used for encrypting/decrypting data stored in the internal
 *  database. If omitted, data will be stored in plaintext.
 */
class PlacesConnection(path: String, encryption_key: String? = null) : PlacesAPI, AutoCloseable {
    private var handle: AtomicLong = AtomicLong(0)
    private var interruptHandle: InterruptHandle

    init {
        try {
            handle.set(rustCall { error ->
                LibPlacesFFI.INSTANCE.places_connection_new(path, encryption_key, error)
            })
        } catch (e: InternalPanic) {

            // Places Rust library does not yet support schema migrations; as a very temporary quick
            // fix to avoid crashes of our upstream consumers, let's delete the database file
            // entirely and try again.
            // FIXME https://github.com/mozilla/application-services/issues/438
            if (e.message != "sorry, no upgrades yet - delete your db!") {
                throw e
            }

            File(path).delete()

            handle.set(rustCall { error ->
                LibPlacesFFI.INSTANCE.places_connection_new(path, encryption_key, error)
            })
        }
        try {
            interruptHandle = InterruptHandle(rustCall { err ->
                LibPlacesFFI.INSTANCE.places_new_interrupt_handle(this.handle.get(), err)
            }!!)
        } catch (e: Throwable) {
            rustCall { error ->
                LibPlacesFFI.INSTANCE.places_connection_destroy(this.handle.getAndSet(0), error)
            }
            throw e
        }
    }

    @Synchronized
    override fun close() {
        val handle = this.handle.getAndSet(0L)
        if (handle != 0L) {
            rustCall { error ->
                LibPlacesFFI.INSTANCE.places_connection_destroy(handle, error)
            }
        }
        interruptHandle.close()
    }

    override fun noteObservation(data: VisitObservation) {
        val json = data.toJSON().toString()
        rustCall { error ->
            LibPlacesFFI.INSTANCE.places_note_observation(this.handle.get(), json, error)
        }
    }

    override fun queryAutocomplete(query: String, limit: Int): List<SearchResult> {
        val json = rustCallForString { error ->
            LibPlacesFFI.INSTANCE.places_query_autocomplete(this.handle.get(), query, limit, error)
        }
        return SearchResult.fromJSONArray(json)
    }

    override fun matchUrl(query: String): String? {
        // Can't use rustCallForString if we return null on success. Possibly worth splitting
        // into a rustCallForOptString or something, but I'll wait until we need it again.
        val urlPtr = rustCall { error ->
            LibPlacesFFI.INSTANCE.places_match_url(this.handle.get(), query, error)
        }
        try {
            return urlPtr?.getString(0, "utf-8")
        } finally {
            urlPtr?.let { LibPlacesFFI.INSTANCE.places_destroy_string(it) }
        }
    }

    override fun getVisited(urls: List<String>): List<Boolean> {
        // Note urlStrings has a potential footgun in that StringArray has a `size()` method
        // which returns the size *in bytes*. Hence us using urls.size (which is an element count)
        // for the actual number of urls!
        val urlStrings = StringArray(urls.toTypedArray(), "utf8")
        val byteBuffer = ByteBuffer.allocateDirect(urls.size)
        byteBuffer.order(ByteOrder.nativeOrder())
        rustCall { error ->
            val bufferPtr = Native.getDirectBufferPointer(byteBuffer)
            LibPlacesFFI.INSTANCE.places_get_visited(
                    this.handle.get(),
                    urlStrings, urls.size,
                    bufferPtr, urls.size,
                    error
            )
        }
        val result = ArrayList<Boolean>(urls.size)
        for (index in 0 until urls.size) {
            val wasVisited = byteBuffer.get(index)
            if (wasVisited != 0.toByte() && wasVisited != 1.toByte()) {
                throw java.lang.RuntimeException(
                        "Places bug! Memory corruption possible! Report me!")
            }
            result.add(wasVisited == 1.toByte())
        }
        return result
    }

    override fun getVisitedUrlsInRange(start: Long, end: Long, includeRemote: Boolean): List<String> {
        val urlsJson = rustCallForString { error ->
            val incRemoteArg: Byte = if (includeRemote) { 1 } else { 0 }
            LibPlacesFFI.INSTANCE.places_get_visited_urls_in_range(
                    this.handle.get(), start, end, incRemoteArg, error)
        }
        val arr = JSONArray(urlsJson)
        val result = mutableListOf<String>();
        for (idx in 0 until arr.length()) {
            result.add(arr.getString(idx))
        }
        return result
    }

    override fun deletePlace(url: String) {
        rustCall { error ->
            LibPlacesFFI.INSTANCE.places_delete_place(
                    this.handle.get(), url, error)
        }
    }

    override fun deleteVisit(url: String, visitTimestamp: Long) {
        rustCall { error ->
            LibPlacesFFI.INSTANCE.places_delete_visit(
                    this.handle.get(), url, visitTimestamp, error)
        }
    }

    override fun deleteVisitsSince(since: Long) {
        deleteVisitsBetween(since, Long.MAX_VALUE)
    }

    override fun deleteVisitsBetween(startTime: Long, endTime: Long) {
        rustCall { error ->
            LibPlacesFFI.INSTANCE.places_delete_visits_between(
                    this.handle.get(), startTime, endTime, error)
        }
    }

    override fun sync(syncInfo: SyncAuthInfo) {
        rustCall { error ->
            LibPlacesFFI.INSTANCE.sync15_history_sync(
                    this.handle.get(),
                    syncInfo.kid,
                    syncInfo.fxaAccessToken,
                    syncInfo.syncKey,
                    syncInfo.tokenserverURL,
                    error
            )
        }
    }

    override fun interrupt() {
        this.interruptHandle.interrupt()
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

/**
 * A class for providing the auth-related information needed to sync.
 * Note that this has the same shape as `SyncUnlockInfo` from logins - we
 * probably want a way of sharing these.
 */
class SyncAuthInfo (
    val kid: String,
    val fxaAccessToken: String,
    val syncKey: String,
    val tokenserverURL: String
)

/**
 * An API for interacting with Places.
 */
interface PlacesAPI {
    /**
     * Record a visit to a URL, or update meta information about page URL. See [VisitObservation].
     */
    fun noteObservation(data: VisitObservation)

    /**
     * A way to search the internal database tailored for autocompletion purposes.
     *
     * @param query a string to match results against.
     * @param limit a maximum number of results to retrieve.
     * @return a list of [SearchResult] matching the [query], in arbitrary order.
     */
    fun queryAutocomplete(query: String, limit: Int): List<SearchResult>

    /**
     * See if a url that's sufficiently close to `search` exists in
     * the database.
     *
     * @param query the search string
     * @return If no url exists, returns null. If one exists, it returns the next
     *         portion of it that definitely matches (where portion is defined
     *         something like 'complete origin or path segment')
     */
    fun matchUrl(query: String): String?

    /**
     * Maps a list of page URLs to a list of booleans indicating if each URL was visited.
     * @param urls a list of page URLs about which "visited" information is being requested.
     * @return a list of booleans indicating visited status of each
     * corresponding page URI from [urls].
     */
    fun getVisited(urls: List<String>): List<Boolean>

    /**
     * Returns a list of visited URLs for a given time range.
     *
     * @param start beginning of the range, unix timestamp in milliseconds.
     * @param end end of the range, unix timestamp in milliseconds.
     * @param includeRemote boolean flag indicating whether or not to include remote visits. A visit
     *  is (roughly) considered remote if it didn't originate on the current device.
     */
    fun getVisitedUrlsInRange(start: Long, end: Long = Long.MAX_VALUE, includeRemote: Boolean = true): List<String>

    /**
     * Deletes all information about the given URL. If the place has previously
     * been synced, a tombstone will be written to the sync server, meaning
     * the place should be deleted on all synced devices.
     *
     * The exception to this is if the place is duplicated on the sync server
     * (duplicate server-side places are a form of corruption), in which case
     * only the place whose GUID corresponds to the local GUID will be
     * deleted. This is (hopefully) rare, and sadly there is not much we can
     * do about it. It indicates a client-side bug that occurred at some
     * point in the past.
     *
     * @param url the url to be removed.
     */
    fun deletePlace(url: String)

    /**
     * Deletes all visits which occurred since the specified time. If the
     * deletion removes the last visit for a place, the place itself will also
     * be removed (and if the place has been synced, the deletion of the
     * place will also be synced)
     *
     * @param start time for the deletion, unix timestamp in milliseconds.
     */
    fun deleteVisitsSince(since: Long)

    /**
     * Equivalent to deleteVisitsSince, but takes an `endTime` as well.
     *
     * Timestamps are in milliseconds since the unix epoch.
     *
     * See documentation for deleteVisitsSince for caveats.
     *
     * @param startTime Inclusive beginning of the time range to delete.
     * @param endTime Inclusive end of the time range to delete.
     */
    fun deleteVisitsBetween(startTime: Long, endTime: Long)

    /**
     * Delete the single visit that occurred at the provided timestamp.
     *
     * Note that this will not delete the visit on another device, unless this is the last
     * remaining visit of that URL that this device is aware of.
     *
     * However, it should prevent this visit from being inserted again.
     *
     * @param url The URL of the place to delete.
     * @param visitTimestamp The timestamp of the visit to delete, in MS since the unix epoch
     */
    fun deleteVisit(url: String, visitTimestamp: Long)

    /**
     * Syncs the history store.
     *
     * Note that this function blocks until the sync is complete, which may
     * take some time due to the network etc. Because only 1 thread can be
     * using a PlacesAPI at a time, it is recommended, but not enforced, that
     * you use a separate PlacesAPI instance purely for syncing.
     *
     */
    fun sync(syncInfo: SyncAuthInfo)

    /**
     * Interrupt ongoing operations running on a separate thread.
     */
    fun interrupt()
}

internal class InterruptHandle internal constructor(raw: RawPlacesInterruptHandle): AutoCloseable {
    // We synchronize all accesses, so this probably doesn't need AtomicReference.
    private val handle: AtomicReference<RawPlacesInterruptHandle?> = AtomicReference(raw)

    @Synchronized
    override fun close() {
        val toFree = handle.getAndSet(null)
        if (toFree != null) {
            LibPlacesFFI.INSTANCE.places_interrupt_handle_destroy(toFree)
        }
    }

    @Synchronized
    fun interrupt() {
        handle.get()?.let {
            val e = RustError.ByReference()
            LibPlacesFFI.INSTANCE.places_interrupt(it, e)
            if (e.isFailure()) {
                throw e.intoException()
            }
        }
    }
}


open class PlacesException(msg: String): Exception(msg)
open class InternalPanic(msg: String): PlacesException(msg)
open class UrlParseFailed(msg: String): PlacesException(msg)
open class InvalidPlaceInfo(msg: String): PlacesException(msg)
open class PlacesConnectionBusy(msg: String): PlacesException(msg)
open class OperationInterrupted(msg: String): PlacesException(msg)

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
