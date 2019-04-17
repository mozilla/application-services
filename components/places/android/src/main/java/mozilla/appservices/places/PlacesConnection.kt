/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.places

import com.sun.jna.Native
import com.sun.jna.Pointer
import com.sun.jna.StringArray
import mozilla.appservices.support.toNioDirectBuffer
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.atomic.AtomicLong
import java.util.concurrent.atomic.AtomicReference
import java.lang.ref.WeakReference

/**
 * An implementation of a [PlacesManager] backed by a Rust Places library.
 *
 * This type, as well as all connection types, are thread safe (they perform locking internally
 * where necessary).
 *
 * @param path an absolute path to a file that will be used for the internal database.
 * @param encryptionKey an optional key used for encrypting/decrypting data stored in the internal
 *  database. If omitted, data will be stored in plaintext.
 */
class PlacesApi(path: String, encryptionKey: String? = null) : PlacesManager, AutoCloseable {
    private var handle: AtomicLong = AtomicLong(0)
    private var writeConn: PlacesWriterConnection

    init {
        handle.set(rustCall(this) { error ->
            LibPlacesFFI.INSTANCE.places_api_new(path, encryptionKey, error)
        })
        writeConn = PlacesWriterConnection(rustCall(this) { error ->
            LibPlacesFFI.INSTANCE.places_connection_new(handle.get(), READ_WRITE, error)
        }, this)
    }

    companion object {
        // These numbers come from `places::db::ConnectionType`
        private const val READ_ONLY: Int = 1
        private const val READ_WRITE: Int = 2
    }

    override fun openReader(): PlacesReaderConnection {
        val connHandle = rustCall(this) { error ->
            LibPlacesFFI.INSTANCE.places_connection_new(handle.get(), READ_ONLY, error)
        }
        return PlacesReaderConnection(connHandle)
    }

    override fun getWriter(): PlacesWriterConnection {
        return writeConn
    }

    @Synchronized
    override fun close() {
        // Take the write connection's handle and clear its reference to us.
        val writeHandle = this.writeConn.takeHandle()
        this.writeConn.apiRef.clear()
        val handle = this.handle.getAndSet(0L)
        if (handle != 0L) {
            if (writeHandle != 0L) {
                try {
                    rustCall(this) { err ->
                        LibPlacesFFI.INSTANCE.places_api_return_write_conn(handle, writeHandle, err)
                    }
                } catch (e: PlacesException) {
                    // Ignore it.
                }
            }
            rustCall(this) { error ->
                LibPlacesFFI.INSTANCE.places_api_destroy(handle, error)
            }
        }
    }

    override fun syncHistory(syncInfo: SyncAuthInfo) {
        rustCall(this) { error ->
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

    override fun syncBookmarks(syncInfo: SyncAuthInfo) {
        rustCall(this) { error ->
            LibPlacesFFI.INSTANCE.sync15_bookmarks_sync(
                    this.handle.get(),
                    syncInfo.kid,
                    syncInfo.fxaAccessToken,
                    syncInfo.syncKey,
                    syncInfo.tokenserverURL,
                    error
            )
        }
    }
}

internal inline fun <U> rustCall(syncOn: Any, callback: (RustError.ByReference) -> U): U {
    synchronized(syncOn) {
        val e = RustError.ByReference()
        val ret: U = callback(e)
        if (e.isFailure()) {
            throw e.intoException()
        } else {
            return ret
        }
    }
}

@Suppress("TooGenericExceptionCaught")
open class PlacesConnection internal constructor(connHandle: Long) : InterruptibleConnection, AutoCloseable {
    protected var handle: AtomicLong = AtomicLong(0)
    protected var interruptHandle: InterruptHandle

    init {
        handle.set(connHandle)
        try {
            interruptHandle = InterruptHandle(rustCall { err ->
                LibPlacesFFI.INSTANCE.places_new_interrupt_handle(connHandle, err)
            }!!)
        } catch (e: Throwable) {
            rustCall { error ->
                LibPlacesFFI.INSTANCE.places_connection_destroy(this.handle.getAndSet(0), error)
            }
            throw e
        }
    }

    @Synchronized
    protected fun destroy() {
        val handle = this.handle.getAndSet(0L)
        if (handle != 0L) {
            rustCall { error ->
                LibPlacesFFI.INSTANCE.places_connection_destroy(handle, error)
            }
        }
        interruptHandle.close()
    }

    @Synchronized
    override fun close() {
        destroy()
    }

    override fun interrupt() {
        this.interruptHandle.interrupt()
    }

    internal inline fun <U> rustCall(callback: (RustError.ByReference) -> U): U {
        return rustCall(this, callback)
    }

    @Suppress("TooGenericExceptionThrown")
    internal inline fun rustCallForString(callback: (RustError.ByReference) -> Pointer?): String {
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
 * An implementation of a [ReadableHistoryConnection], used for read-only
 * access to places APIs.
 *
 * This class is thread safe.
 */
open class PlacesReaderConnection internal constructor(connHandle: Long) :
        PlacesConnection(connHandle),
        ReadableHistoryConnection,
        ReadableBookmarksConnection {
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
        val result = mutableListOf<String>()
        for (idx in 0 until arr.length()) {
            result.add(arr.getString(idx))
        }
        return result
    }

    override fun getVisitInfos(start: Long, end: Long): List<VisitInfo> {
        val infoBuffer = rustCall { error ->
            LibPlacesFFI.INSTANCE.places_get_visit_infos(
                    this.handle.get(), start, end, error)
        }
        try {
            val infos = MsgTypes.HistoryVisitInfos.parseFrom(infoBuffer.asCodedInputStream()!!)
            return VisitInfo.fromMessage(infos)
        } finally {
            LibPlacesFFI.INSTANCE.places_destroy_bytebuffer(infoBuffer)
        }
    }

    override fun getBookmark(guid: String): BookmarkTreeNode? {
        val rustBuf = rustCall { err ->
            LibPlacesFFI.INSTANCE.bookmarks_get_by_guid(this.handle.get(), guid, 0.toByte(), err)
        }
        try {
            return rustBuf.asCodedInputStream()?.let { stream ->
                unpackProtobuf(MsgTypes.BookmarkNode.parseFrom(stream))
            }
        } finally {
            LibPlacesFFI.INSTANCE.places_destroy_bytebuffer(rustBuf)
        }
    }

    override fun getBookmarksTree(rootGUID: String, recursive: Boolean): BookmarkTreeNode? {
        val rustBuf = rustCall { err ->
            if (recursive) {
                LibPlacesFFI.INSTANCE.bookmarks_get_tree(this.handle.get(), rootGUID, err)
            } else {
                LibPlacesFFI.INSTANCE.bookmarks_get_by_guid(this.handle.get(), rootGUID, 1.toByte(), err)
            }
        }
        try {
            return rustBuf.asCodedInputStream()?.let { stream ->
                unpackProtobuf(MsgTypes.BookmarkNode.parseFrom(stream))
            }
        } finally {
            LibPlacesFFI.INSTANCE.places_destroy_bytebuffer(rustBuf)
        }
    }

    override fun getBookmarksWithURL(url: String): List<BookmarkItem> {
        val rustBuf = rustCall { err ->
            LibPlacesFFI.INSTANCE.bookmarks_get_all_with_url(this.handle.get(), url, err)
        }

        try {
            val message = MsgTypes.BookmarkNodeList.parseFrom(rustBuf.asCodedInputStream()!!)
            return unpackProtobufItemList(message)
        } finally {
            LibPlacesFFI.INSTANCE.places_destroy_bytebuffer(rustBuf)
        }
    }

    override fun searchBookmarks(query: String, limit: Int): List<BookmarkItem> {
        val rustBuf = rustCall { err ->
            LibPlacesFFI.INSTANCE.bookmarks_search(this.handle.get(), query, limit, err)
        }

        try {
            val message = MsgTypes.BookmarkNodeList.parseFrom(rustBuf.asCodedInputStream()!!)
            return unpackProtobufItemList(message)
        } finally {
            LibPlacesFFI.INSTANCE.places_destroy_bytebuffer(rustBuf)
        }
    }
}

/**
 * An implementation of a [WritableHistoryConnection], use for read or write
 * access to the Places APIs.
 *
 * This class is thread safe.
 */
@Suppress("TooManyFunctions")
class PlacesWriterConnection internal constructor(connHandle: Long, api: PlacesApi) :
        PlacesReaderConnection(connHandle),
        WritableHistoryConnection,
        WritableBookmarksConnection {
    // The reference to our PlacesAPI. Mostly used to know how to handle getting closed.
    val apiRef = WeakReference(api)
    override fun noteObservation(data: VisitObservation) {
        val json = data.toJSON().toString()
        rustCall { error ->
            LibPlacesFFI.INSTANCE.places_note_observation(this.handle.get(), json, error)
        }
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

    override fun wipeLocal() {
        rustCall { error ->
            LibPlacesFFI.INSTANCE.places_wipe_local(this.handle.get(), error)
        }
    }

    override fun runMaintenance() {
        rustCall { error ->
            LibPlacesFFI.INSTANCE.places_run_maintenance(this.handle.get(), error)
        }
    }

    override fun pruneDestructively() {
        rustCall { error ->
            LibPlacesFFI.INSTANCE.places_prune_destructively(this.handle.get(), error)
        }
    }

    override fun deleteEverything() {
        rustCall { error ->
            LibPlacesFFI.INSTANCE.places_delete_everything(this.handle.get(), error)
        }
    }

    override fun deleteBookmarkNode(guid: String): Boolean {
        val existedByte = rustCall { error ->
            LibPlacesFFI.INSTANCE.bookmarks_delete(this.handle.get(), guid, error)
        }
        return existedByte.toInt() != 0
    }

    // Does the shared insert work, takes the position just because
    // its a little tedious to type out setting it
    private fun doInsert(builder: MsgTypes.BookmarkNode.Builder, position: Int?): String {
        position?.let { builder.setPosition(position) }
        val buf = builder.build()
        val (nioBuf, len) = buf.toNioDirectBuffer()
        return rustCallForString { err ->
            val ptr = Native.getDirectBufferPointer(nioBuf)
            LibPlacesFFI.INSTANCE.bookmarks_insert(this.handle.get(), ptr, len, err)
        }
    }

    override fun createFolder(parentGUID: String, title: String, position: Int?): String {
        val builder = MsgTypes.BookmarkNode.newBuilder()
                .setNodeType(BookmarkType.Folder.value)
                .setParentGuid(parentGUID)
                .setTitle(title)
        return this.doInsert(builder, position)
    }

    override fun createSeparator(parentGUID: String, position: Int?): String {
        val builder = MsgTypes.BookmarkNode.newBuilder()
                .setNodeType(BookmarkType.Separator.value)
                .setParentGuid(parentGUID)
        return this.doInsert(builder, position)
    }

    override fun createBookmarkItem(parentGUID: String, url: String, title: String, position: Int?): String {
        val builder = MsgTypes.BookmarkNode.newBuilder()
                .setNodeType(BookmarkType.Bookmark.value)
                .setParentGuid(parentGUID)
                .setUrl(url)
                .setTitle(title)
        return this.doInsert(builder, position)
    }

    override fun updateBookmark(guid: String, info: BookmarkUpdateInfo) {
        val buf = info.toProtobuf(guid)
        val (nioBuf, len) = buf.toNioDirectBuffer()
        rustCall { err ->
            val ptr = Native.getDirectBufferPointer(nioBuf)
            LibPlacesFFI.INSTANCE.bookmarks_update(this.handle.get(), ptr, len, err)
        }
    }

    @Synchronized
    override fun close() {
        // If our API is still around, do nothing.
        if (apiRef.get() == null) {
            // Otherwise, it must have gotten GCed without calling close() :(
            // So we go through the non-writer connection destructor.
            destroy()
        }
    }

    @Synchronized
    internal fun takeHandle(): PlacesConnectionHandle {
        val handle = this.handle.getAndSet(0L)
        interruptHandle.close()
        return handle
    }
}

/**
 * A class for providing the auth-related information needed to sync.
 * Note that this has the same shape as `SyncUnlockInfo` from logins - we
 * probably want a way of sharing these.
 */
class SyncAuthInfo(
    val kid: String,
    val fxaAccessToken: String,
    val syncKey: String,
    val tokenserverURL: String
)

/**
 * An API for interacting with Places. This is the top-level entry-point, and
 * exposes functions which return lower-level objects with the core
 * functionality.
 */
interface PlacesManager {
    /**
     * Open a reader connection.
     */
    fun openReader(): ReadableHistoryConnection

    /**
     * Get a reference to the writer connection.
     *
     * This should always return the same object.
     */
    fun getWriter(): WritableHistoryConnection

    /**
     * Syncs the places history store.
     *
     * Note that this function blocks until the sync is complete, which may
     * take some time due to the network etc. Because only 1 thread can be
     * using a PlacesAPI at a time, it is recommended, but not enforced, that
     * you have all connections you intend using open before calling this.
     */
    fun syncHistory(syncInfo: SyncAuthInfo)

    /**
     * Syncs the places bookmarks store.
     *
     * Note that this function blocks until the sync is complete, which may
     * take some time due to the network etc. Because only 1 thread can be
     * using a PlacesAPI at a time, it is recommended, but not enforced, that
     * you have all connections you intend using open before calling this.
     */
    fun syncBookmarks(syncInfo: SyncAuthInfo)
}

interface InterruptibleConnection : AutoCloseable {
    /**
     * Interrupt ongoing operations running on a separate thread.
     */
    fun interrupt()
}

interface ReadableHistoryConnection : InterruptibleConnection {
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
     * Get detailed information about all visits that occurred in the
     * given time range.
     *
     * @param start The (inclusive) start time to bound the query.
     * @param end The (inclusive) end time to bound the query.
     */
    fun getVisitInfos(start: Long, end: Long = Long.MAX_VALUE): List<VisitInfo>
}

interface WritableHistoryConnection : ReadableHistoryConnection {
    /**
     * Record a visit to a URL, or update meta information about page URL. See [VisitObservation].
     */
    fun noteObservation(data: VisitObservation)

    /**
     * Deletes all history visits, without recording tombstones.
     *
     * That is, these deletions will not be synced. Any changes which were
     * pending upload on the next sync are discarded and will be lost.
     */
    fun wipeLocal()

    /**
     * Run periodic database maintenance. This might include, but is not limited
     * to:
     *
     * - `VACUUM`ing.
     * - Requesting that the indices in our tables be optimized.
     * - Expiring irrelevant history visits.
     * - Periodic repair or deletion of corrupted records.
     * - etc.
     *
     * It should be called at least once a day, but this is merely a
     * recommendation and nothing too dire should happen if it is not
     * called.
     */
    fun runMaintenance()

    /**
     * Aggressively prune history visits. These deletions are not intended
     * to be synced, however due to the way history sync works, this can
     * still cause data loss.
     *
     * As a result, this should only be called if a low disk space
     * notification is received from the OS, and things like the network
     * cache have already been cleared.
     */
    fun pruneDestructively()

    /**
     * Delete everything locally.
     *
     * This will not delete visits from remote devices, however it will
     * prevent them from trickling in over time when future syncs occur.
     *
     * The difference between this and wipeLocal is that wipeLocal does
     * not prevent the deleted visits from returning. For wipeLocal,
     * the visits will return on the next full sync (which may be
     * arbitrarially far in the future), wheras items which were
     * deleted by deleteEverything (or potentially could have been)
     * should not return.
     */
    fun deleteEverything()

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
}

class InterruptHandle internal constructor(raw: RawPlacesInterruptHandle) : AutoCloseable {
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

open class PlacesException(msg: String) : Exception(msg)
open class InternalPanic(msg: String) : PlacesException(msg)
open class UrlParseFailed(msg: String) : PlacesException(msg)
open class PlacesConnectionBusy(msg: String) : PlacesException(msg)
open class OperationInterrupted(msg: String) : PlacesException(msg)

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

private val intToVisitType: Map<Int, VisitType> = VisitType.values().associateBy(VisitType::type)

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

private fun stringOrNull(jsonObject: JSONObject, key: String): String? {
    return try {
        jsonObject.getString(key)
    } catch (e: JSONException) {
        null
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
            return SearchResult(
                searchString = jsonObject.getString("search_string"),
                url = jsonObject.getString("url"),
                title = jsonObject.getString("title"),
                frecency = jsonObject.getLong("frecency"),
                iconUrl = stringOrNull(jsonObject, "icon_url")
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

/**
 * Information about a history visit. Returned by `PlacesAPI.getVisitInfos`.
 */
data class VisitInfo(
    /**
     * The URL of the page that was visited.
     */
    val url: String,

    /**
     * The title of the page that was visited, if known.
     */
    val title: String?,

    /**
     * The time the page was visited in integer milliseconds since the unix epoch.
     */
    val visitTime: Long,

    /**
     * What the transition type of the visit is.
     */
    val visitType: VisitType
) {
    companion object {
        internal fun fromMessage(msg: MsgTypes.HistoryVisitInfos): List<VisitInfo> {
            return msg.infosList.map {
                VisitInfo(url = it.url,
                    title = it.title,
                    visitTime = it.timestamp,
                    visitType = intToVisitType[it.visitType]!!)
            }
        }
    }
}
