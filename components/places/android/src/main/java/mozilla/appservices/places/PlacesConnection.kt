/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.places

import com.sun.jna.Native
import com.sun.jna.Pointer
import mozilla.appservices.places.uniffi.ConnectionType
import mozilla.appservices.places.uniffi.DocumentType
import mozilla.appservices.places.uniffi.FrecencyThresholdOption
import mozilla.appservices.places.uniffi.PlacesException
import mozilla.appservices.places.uniffi.HistoryHighlight
import mozilla.appservices.places.uniffi.HistoryHighlightWeights
import mozilla.appservices.places.uniffi.HistoryMetadata
import mozilla.appservices.places.uniffi.HistoryMetadataObservation
import mozilla.appservices.places.uniffi.TopFrecentSiteInfo
import mozilla.appservices.places.uniffi.PlacesApi as UniffiPlacesApi
import mozilla.appservices.places.uniffi.PlacesConnection as UniffiPlacesConnection
import mozilla.appservices.places.uniffi.placesApiNew
import mozilla.appservices.places.uniffi.VisitObservation
import mozilla.appservices.places.uniffi.HistoryVisitInfo
import mozilla.appservices.places.uniffi.HistoryVisitInfosWithBound
import mozilla.appservices.support.native.toNioDirectBuffer
import mozilla.appservices.sync15.SyncTelemetryPing
import mozilla.components.service.glean.private.CounterMetricType
import mozilla.components.service.glean.private.LabeledMetricType
import org.json.JSONObject
import java.lang.ref.WeakReference
import java.util.concurrent.atomic.AtomicLong
import java.util.concurrent.atomic.AtomicReference
import org.mozilla.appservices.places.GleanMetrics.PlacesManager as PlacesManagerMetrics

typealias Url = String

/**
 * An implementation of a [PlacesManager] backed by a Rust Places library.
 *
 * This type, as well as all connection types, are thread safe (they perform locking internally
 * where necessary).
 *
 * @param path an absolute path to a file that will be used for the internal database.
 */
class PlacesApi(path: String) : PlacesManager, AutoCloseable {
    // As a temp work-around while we uniffi, we actually have 2 references to a PlacesApi - one
    // via a handle in the old-school HandleMap, and the other via uniffi.
    private var handle: AtomicLong = AtomicLong(0)
    private var api: UniffiPlacesApi
    private var writeConn: PlacesWriterConnection

    init {
        handle.set(
            rustCall(this) { error ->
                LibPlacesFFI.INSTANCE.places_api_new(path, error)
            }
        )
        // as per https://github.com/mozilla/uniffi-rs/pull/1063, there was some
        // pushback on allowing this to actually be a constructor, so it's a global
        // function instead :(
        api = placesApiNew(path)

        // Our connections also live a double-life.
        val connHandle = rustCall(this) { error ->
            LibPlacesFFI.INSTANCE.places_connection_new(handle.get(), READ_WRITE, error)
        }
        val uniffiConnection = api.newConnection(ConnectionType.READ_WRITE)
        writeConn = PlacesWriterConnection(connHandle, uniffiConnection, this)
    }

    companion object {
        // These numbers come from `places::db::ConnectionType`
        private const val READ_ONLY: Int = 1
        private const val READ_WRITE: Int = 2
    }

    override fun registerWithSyncManager() {
        rustCall(this) { error ->
            LibPlacesFFI.INSTANCE.places_api_register_with_sync_manager(handle.get(), error)
        }
    }

    override fun openReader(): PlacesReaderConnection {
        // This is starting to get messy - we actually end up with 2 different raw uniffi
        // connections inside a single PlacesReaderConnection. WCPGW?
        val connHandle = rustCall(this) { error ->
            LibPlacesFFI.INSTANCE.places_connection_new(handle.get(), READ_ONLY, error)
        }
        val conn = api.newConnection(ConnectionType.READ_ONLY)
        return PlacesReaderConnection(connHandle, conn)
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

    override fun syncHistory(syncInfo: SyncAuthInfo): SyncTelemetryPing {
        val pingJSONString = rustCallForString(this) { error ->
            LibPlacesFFI.INSTANCE.sync15_history_sync(
                this.handle.get(),
                syncInfo.kid,
                syncInfo.fxaAccessToken,
                syncInfo.syncKey,
                syncInfo.tokenserverURL,
                error
            )
        }
        return SyncTelemetryPing.fromJSONString(pingJSONString)
    }

    override fun syncBookmarks(syncInfo: SyncAuthInfo): SyncTelemetryPing {
        val pingJSONString = rustCallForString(this) { error ->
            LibPlacesFFI.INSTANCE.sync15_bookmarks_sync(
                this.handle.get(),
                syncInfo.kid,
                syncInfo.fxaAccessToken,
                syncInfo.syncKey,
                syncInfo.tokenserverURL,
                error
            )
        }
        return SyncTelemetryPing.fromJSONString(pingJSONString)
    }

    override fun importBookmarksFromFennec(path: String): JSONObject {
        val json = rustCallForString(this) { error ->
            LibPlacesFFI.INSTANCE.places_bookmarks_import_from_fennec(this.handle.get(), path, error)
        }
        return JSONObject(json)
    }

    override fun importPinnedSitesFromFennec(path: String): List<BookmarkItem> {
        val rustBuf = rustCall(this) { error ->
            LibPlacesFFI.INSTANCE.places_pinned_sites_import_from_fennec(
                this.handle.get(), path, error
            )
        }

        try {
            val message = MsgTypes.BookmarkNodeList.parseFrom(rustBuf.asCodedInputStream()!!)
            return unpackProtobufItemList(message)
        } finally {
            LibPlacesFFI.INSTANCE.places_destroy_bytebuffer(rustBuf)
        }
    }

    override fun importVisitsFromFennec(path: String): JSONObject {
        val json = rustCallForString(this) { error ->
            LibPlacesFFI.INSTANCE.places_history_import_from_fennec(this.handle.get(), path, error)
        }
        return JSONObject(json)
    }

    override fun resetHistorySyncMetadata() {
        rustCall(this) { error ->
            LibPlacesFFI.INSTANCE.places_reset(this.handle.get(), error)
        }
    }

    override fun resetBookmarkSyncMetadata() {
        rustCall(this) { error ->
            LibPlacesFFI.INSTANCE.bookmarks_reset(this.handle.get(), error)
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

@Suppress("TooGenericExceptionThrown")
internal inline fun rustCallForString(syncOn: Any, callback: (RustError.ByReference) -> Pointer?): String {
    val cstring = rustCall(syncOn, callback)
        ?: throw RuntimeException(
            "Bug: Don't use this function when you can return" +
                " null on success."
        )
    try {
        return cstring.getString(0, "utf8")
    } finally {
        LibPlacesFFI.INSTANCE.places_destroy_string(cstring)
    }
}

internal inline fun rustCallForOptString(syncOn: Any, callback: (RustError.ByReference) -> Pointer?): String? {
    val cstring = rustCall(syncOn, callback)
    try {
        return cstring?.getString(0, "utf8")
    } finally {
        cstring?.let { LibPlacesFFI.INSTANCE.places_destroy_string(it) }
    }
}

@Suppress("TooGenericExceptionCaught")
open class PlacesConnection internal constructor(connHandle: Long, uniffiConn: UniffiPlacesConnection) : InterruptibleConnection, AutoCloseable {
    // As a temp work-around while we uniffi, we actually have 2 references to a PlacesConnection- one
    // via a handle in the old-school HandleMap, and the other via uniffi.
    // Each method here will use one or the other, depending on whether it's been uniffi'd or not.
    protected var handle: AtomicLong = AtomicLong(0)
    protected var conn: UniffiPlacesConnection
    protected var interruptHandle: InterruptHandle

    init {
        handle.set(connHandle)
        conn = uniffiConn
        try {
            interruptHandle = InterruptHandle(
                rustCall { err ->
                    LibPlacesFFI.INSTANCE.places_new_interrupt_handle(connHandle, err)
                }!!
            )
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
        conn.destroy()
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

    internal inline fun rustCallForString(callback: (RustError.ByReference) -> Pointer?): String {
        return rustCallForString(this, callback)
    }

    internal inline fun rustCallForOptString(callback: (RustError.ByReference) -> Pointer?): String? {
        return rustCallForOptString(this, callback)
    }
}

/**
 * An implementation of a [ReadableHistoryConnection], used for read-only
 * access to places APIs.
 *
 * This class is thread safe.
 */
open class PlacesReaderConnection internal constructor(connHandle: Long, conn: UniffiPlacesConnection) :
    PlacesConnection(connHandle, conn),
    ReadableHistoryConnection,
    ReadableHistoryMetadataConnection,
    ReadableBookmarksConnection {
    override fun queryAutocomplete(query: String, limit: Int): List<SearchResult> {
        val resultBuffer = rustCall { error ->
            LibPlacesFFI.INSTANCE.places_query_autocomplete(this.handle.get(), query, limit, error)
        }
        try {
            val results = MsgTypes.SearchResultList.parseFrom(resultBuffer.asCodedInputStream()!!)
            return SearchResult.fromCollectionMessage(results)
        } finally {
            LibPlacesFFI.INSTANCE.places_destroy_bytebuffer(resultBuffer)
        }
    }

    override fun matchUrl(query: String): String? {
        return rustCallForOptString { error ->
            LibPlacesFFI.INSTANCE.places_match_url(this.handle.get(), query, error)
        }
    }

    override fun getTopFrecentSiteInfos(numItems: Int, frecencyThreshold: FrecencyThresholdOption): List<TopFrecentSiteInfo> {
        return this.conn.getTopFrecentSiteInfos(numItems, frecencyThreshold)
    }

    override fun getVisited(urls: List<String>): List<Boolean> {
        return this.conn.getVisited(urls)
    }

    override fun getVisitedUrlsInRange(start: Long, end: Long, includeRemote: Boolean): List<String> {
        return this.conn.getVisitedUrlsInRange(start, end, includeRemote)
    }

    override fun getVisitInfos(start: Long, end: Long, excludeTypes: List<VisitType>): List<HistoryVisitInfo> {
        readQueryCounters.measure {
            return this.conn.getVisitInfos(start, end, visitTransitionSet(excludeTypes))
        }
    }

    override fun getVisitPage(offset: Long, count: Long, excludeTypes: List<VisitType>): List<HistoryVisitInfo> {
        return this.conn.getVisitPage(offset, count, visitTransitionSet(excludeTypes))
    }

    override fun getVisitPageWithBound(
        bound: Long,
        offset: Long,
        count: Long,
        excludeTypes: List<VisitType>
    ): HistoryVisitInfosWithBound {
        return this.conn.getVisitPageWithBound(offset, bound, count, visitTransitionSet(excludeTypes))
    }

    override fun getVisitCount(excludeTypes: List<VisitType>): Long {
        return this.conn.getVisitCount(visitTransitionSet(excludeTypes))
    }

    override suspend fun getLatestHistoryMetadataForUrl(url: Url): HistoryMetadata? {
        return readQueryCounters.measure {
            this.conn.getLatestHistoryMetadataForUrl(url)
        }
    }

    override suspend fun getHistoryMetadataSince(since: Long): List<HistoryMetadata> {
        return readQueryCounters.measure {
            this.conn.getHistoryMetadataSince(since)
        }
    }

    override suspend fun getHistoryMetadataBetween(start: Long, end: Long): List<HistoryMetadata> {
        return readQueryCounters.measure {
            this.conn.getHistoryMetadataBetween(start, end)
        }
    }

    override suspend fun queryHistoryMetadata(query: String, limit: Int): List<HistoryMetadata> {
        return readQueryCounters.measure {
            this.conn.queryHistoryMetadata(query, limit)
        }
    }

    override suspend fun getHighlights(
        weights: HistoryHighlightWeights,
        limit: Int
    ): List<HistoryHighlight> {
        return readQueryCounters.measure {
            this.conn.getHistoryHighlights(weights, limit)
        }
    }

    override fun getBookmark(guid: String): BookmarkTreeNode? {
        readQueryCounters.measure {
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
        readQueryCounters.measure {
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
    }

    override fun getBookmarkUrlForKeyword(keyword: String): String? {
        return rustCallForOptString { error ->
            LibPlacesFFI.INSTANCE.bookmarks_get_url_for_keyword(this.handle.get(), keyword, error)
        }
    }

    override fun searchBookmarks(query: String, limit: Int): List<BookmarkItem> {
        readQueryCounters.measure {
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

    override fun getRecentBookmarks(limit: Int): List<BookmarkItem> {
        readQueryCounters.measure {
            val rustBuf = rustCall { err ->
                LibPlacesFFI.INSTANCE.bookmarks_get_recent(this.handle.get(), limit, err)
            }

            try {
                val message = MsgTypes.BookmarkNodeList.parseFrom(rustBuf.asCodedInputStream()!!)
                return unpackProtobufItemList(message)
            } finally {
                LibPlacesFFI.INSTANCE.places_destroy_bytebuffer(rustBuf)
            }
        }
    }

    private val readQueryCounters: PlacesManagerCounterMetrics by lazy {
        PlacesManagerCounterMetrics(
            PlacesManagerMetrics.readQueryCount,
            PlacesManagerMetrics.readQueryErrorCount
        )
    }
}

fun visitTransitionSet(l: List<VisitType>): Int {
    var res = 0
    for (ty in l) {
        res = res or (1 shl ty.type)
    }
    return res
}

/**
 * An implementation of a [WritableHistoryConnection], use for read or write
 * access to the Places APIs.
 *
 * This class is thread safe.
 */
class PlacesWriterConnection internal constructor(connHandle: Long, conn: UniffiPlacesConnection, api: PlacesApi) :
    PlacesReaderConnection(connHandle, conn),
    WritableHistoryConnection,
    WritableHistoryMetadataConnection,
    WritableBookmarksConnection {
    // The reference to our PlacesAPI. Mostly used to know how to handle getting closed.
    val apiRef = WeakReference(api)
    override fun noteObservation(data: VisitObservation) {
        return writeQueryCounters.measure {
            this.conn.applyObservation(data)
        }
    }

    override fun deleteVisitsFor(url: String) {
        return writeQueryCounters.measure {
            this.conn.deleteVisitsFor(url)
        }
    }

    override fun deleteVisit(url: String, visitTimestamp: Long) {
        return writeQueryCounters.measure {
            this.conn.deleteVisit(url, visitTimestamp)
        }
    }

    override fun deleteVisitsSince(since: Long) {
        deleteVisitsBetween(since, Long.MAX_VALUE)
    }

    override fun deleteVisitsBetween(startTime: Long, endTime: Long) {
        return writeQueryCounters.measure {
            this.conn.deleteVisitsBetween(startTime, endTime)
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
        return writeQueryCounters.measure {
            rustCall { error ->
                LibPlacesFFI.INSTANCE.places_delete_everything(this.handle.get(), error)
            }
        }
    }

    override fun deleteAllBookmarks() {
        return writeQueryCounters.measure {
            rustCall { error ->
                LibPlacesFFI.INSTANCE.bookmarks_delete_everything(this.handle.get(), error)
            }
        }
    }

    override fun deleteBookmarkNode(guid: String): Boolean {
        return writeQueryCounters.measure {
            rustCall { error ->
                val existedByte = LibPlacesFFI.INSTANCE.bookmarks_delete(this.handle.get(), guid, error)
                existedByte.toInt() != 0
            }
        }
    }

    override suspend fun noteHistoryMetadataObservation(observation: HistoryMetadataObservation) {
        // Different types of `HistoryMetadataObservation` are flattened out into a list of values.
        // The other side of this (rust code) is going to deal with missing/absent values. We're just
        // passing them along here.
        // NB: Even though `MsgTypes.HistoryMetadataObservation` has an optional title field, we ignore it here.
        // That's used by consumers which aren't already using the history observation APIs.
        return writeQueryCounters.measure {
            this.conn.noteHistoryMetadataObservation(observation)
        }
    }

    override suspend fun noteHistoryMetadataObservationViewTime(key: HistoryMetadataKey, viewTime: Int) {
        val obs = HistoryMetadataObservation(
            url = key.url,
            searchTerm = key.searchTerm,
            referrerUrl = key.referrerUrl,
            viewTime = viewTime
        )
        noteHistoryMetadataObservation(obs)
    }

    override suspend fun noteHistoryMetadataObservationDocumentType(key: HistoryMetadataKey, documentType: DocumentType) {
        val obs = HistoryMetadataObservation(
            url = key.url,
            searchTerm = key.searchTerm,
            referrerUrl = key.referrerUrl,
            documentType = documentType
        )
        noteHistoryMetadataObservation(obs)
    }

    override suspend fun deleteHistoryMetadataOlderThan(olderThan: Long) {
        return writeQueryCounters.measure {
            this.conn.metadataDeleteOlderThan(olderThan)
        }
    }

    override suspend fun deleteHistoryMetadata(key: HistoryMetadataKey) {
        return writeQueryCounters.measure {
            this.conn.metadataDelete(
                key.url,
                key.referrerUrl,
                key.searchTerm
            )
        }
    }

    // Does the shared insert work, takes the position just because
    // its a little tedious to type out setting it
    private fun doInsert(builder: MsgTypes.BookmarkNode.Builder, position: Int?): String {
        position?.let { builder.setPosition(position) }
        val buf = builder.build()
        val (nioBuf, len) = buf.toNioDirectBuffer()
        writeQueryCounters.measure {
            return rustCallForString { err ->
                val ptr = Native.getDirectBufferPointer(nioBuf)
                LibPlacesFFI.INSTANCE.bookmarks_insert(this.handle.get(), ptr, len, err)
            }
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
        return writeQueryCounters.measure {
            rustCall { err ->
                val ptr = Native.getDirectBufferPointer(nioBuf)
                LibPlacesFFI.INSTANCE.bookmarks_update(this.handle.get(), ptr, len, err)
            }
        }
    }

    override fun acceptResult(searchString: String, url: String) {
        rustCall { error ->
            LibPlacesFFI.INSTANCE.places_accept_result(
                this.handle.get(), searchString, url, error
            )
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

    private val writeQueryCounters: PlacesManagerCounterMetrics by lazy {
        PlacesManagerCounterMetrics(
            PlacesManagerMetrics.writeQueryCount,
            PlacesManagerMetrics.writeQueryErrorCount
        )
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
     * Registers with the sync manager.
     *
     * Call this to enable bookmarks/history syncing functionality
     */
    fun registerWithSyncManager()

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
     * Syncs the places history store, returning a telemetry ping.
     *
     * Note that this function blocks until the sync is complete, which may
     * take some time due to the network etc. Because only 1 thread can be
     * using a PlacesAPI at a time, it is recommended, but not enforced, that
     * you have all connections you intend using open before calling this.
     */
    fun syncHistory(syncInfo: SyncAuthInfo): SyncTelemetryPing

    /**
     * Syncs the places bookmarks store, returning a telemetry ping.
     *
     * Note that this function blocks until the sync is complete, which may
     * take some time due to the network etc. Because only 1 thread can be
     * using a PlacesAPI at a time, it is recommended, but not enforced, that
     * you have all connections you intend using open before calling this.
     */
    fun syncBookmarks(syncInfo: SyncAuthInfo): SyncTelemetryPing

    /**
     * Imports bookmarks from a Fennec `browser.db` database.
     *
     * It has been designed exclusively for non-sync users.
     *
     * @param path Path to the `browser.db` file database.
     * @return JSONObject with import metrics.
     */
    fun importBookmarksFromFennec(path: String): JSONObject

    /**
     * Imports visits from a Fennec `browser.db` database.
     *
     * It has been designed exclusively for non-sync users and should
     * be called before bookmarks import.
     *
     * @param path Path to the `browser.db` file database.
     * @return JSONObject with import metrics.
     */
    fun importVisitsFromFennec(path: String): JSONObject

    /**
     * Returns pinned sites from a Fennec `browser.db` bookmark database.
     *
     * Fennec used to store "pinned websites" as normal bookmarks
     * under an invisible root.
     * During import, this un-syncable root and its children are ignored,
     * so we return the pinned websites separately as a list so
     * Fenix can store them in a collection.
     *
     * @param path Path to the `browser.db` file database.
     * @return A list of pinned websites.
     */
    fun importPinnedSitesFromFennec(path: String): List<BookmarkItem>

    /**
     * Resets all sync metadata for history, including change flags,
     * sync statuses, and last sync time. The next sync after reset
     * will behave the same way as a first sync when connecting a new
     * device.
     *
     * This method only needs to be called when the user disconnects
     * from Sync. There are other times when Places resets sync metadata,
     * but those are handled internally in the Rust code.
     */
    fun resetHistorySyncMetadata()

    /**
     * Resets all sync metadata for bookmarks, including change flags,
     * sync statuses, and last sync time. The next sync after reset
     * will behave the same way as a first sync when connecting a new
     * device.
     *
     * This method only needs to be called when the user disconnects
     * from Sync. There are other times when Places resets sync metadata,
     * but those are handled internally in the Rust code.
     */
    fun resetBookmarkSyncMetadata()
}

interface InterruptibleConnection : AutoCloseable {
    /**
     * Interrupt ongoing operations running on a separate thread.
     */
    fun interrupt()
}

/**
 * This interface exposes the 'read' part of the [HistoryMetadata] storage API.
 */
interface ReadableHistoryMetadataConnection : InterruptibleConnection {
    /**
     * Returns the most recent [HistoryMetadata] for the provided [url].
     *
     * @param url Url to search by.
     * @return [HistoryMetadata] if there's a matching record, `null` otherwise.
     */
    suspend fun getLatestHistoryMetadataForUrl(url: String): HistoryMetadata?

    /**
     * Returns all [HistoryMetadata] where [HistoryMetadata.updatedAt] is greater or equal to [since].
     *
     * @param since Timestmap to search by.
     * @return A `List` of matching [HistoryMetadata], empty if nothing is found.
     */
    suspend fun getHistoryMetadataSince(since: Long): List<HistoryMetadata>

    /**
     * Returns all [HistoryMetadata] where [HistoryMetadata.updatedAt] is between [start] and [end], inclusive.
     *
     * @param start A `start` timestamp.
     * @param end An `end` timestamp.
     * @return A `List` of matching [HistoryMetadata], empty if nothing is found.
     */
    suspend fun getHistoryMetadataBetween(start: Long, end: Long): List<HistoryMetadata>

    /**
     * Searches through [HistoryMetadata] by [query], matching records by [HistoryMetadata.url],
     * [HistoryMetadata.title] and [HistoryMetadata.searchTerm].
     *
     * @param query A search query.
     * @param limit A maximum number of records to return.
     * @return A `List` of matching [HistoryMetadata], empty if nothing is found.
     */
    suspend fun queryHistoryMetadata(query: String, limit: Int): List<HistoryMetadata>

    /**
     * Returns an ordered list of [HistoryHighlight], ranked by their "highlight score".
     * A highlight score takes into account factors listed in [HistoryHighlightWeights].
     *
     * @param weights A set of weights that specify importance of various factors to the highlight score.
     * @param limit A maximum number of records to return.
     * @return A `List` of ranked [HistoryHighlight], empty if no history/metadata is found.
     */
    suspend fun getHighlights(weights: HistoryHighlightWeights, limit: Int): List<HistoryHighlight>
}

/**
 * This interface exposes the 'write' part of the [HistoryMetadata] storage API.
 */
interface WritableHistoryMetadataConnection : ReadableHistoryMetadataConnection {
    /**
     * Record or update metadata information about a URL. See [HistoryMetadataObservation].
     */
    suspend fun noteHistoryMetadataObservation(observation: HistoryMetadataObservation)
    // There's a bit of an impedance mismatch here; `HistoryMetadataKey` is
    // a concept that only exists here and not in the rust. We can iterate on
    // this as the entire "history metadata" requirement evolves.
    suspend fun noteHistoryMetadataObservationViewTime(key: HistoryMetadataKey, viewTime: Int)
    suspend fun noteHistoryMetadataObservationDocumentType(key: HistoryMetadataKey, documentType: DocumentType)

    /**
     * Deletes [HistoryMetadata] with [HistoryMetadata.updatedAt] older than [olderThan].
     *
     * @param olderThan A timestamp to delete records by. Exclusive.
     */
    suspend fun deleteHistoryMetadataOlderThan(olderThan: Long)

    /**
     * Deletes metadata records that match [key].
     *
     * @param key A [HistoryMetadataKey] for which to delete metadata records.
     */
    suspend fun deleteHistoryMetadata(key: HistoryMetadataKey)
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
     * Returns a list of the top frecent site infos limited by the given number of items
     * and frecency threshold sorted by most to least frecent.
     *
     * @param numItems the number of top frecent sites to return in the list.
     * @param frecencyThreshold frecency threshold options for filtering visited sites based on
     * their frecency score.
     * @return a list of the top frecent site infos sorted by most to least frecent.
     */
    fun getTopFrecentSiteInfos(numItems: Int, frecencyThreshold: FrecencyThresholdOption): List<TopFrecentSiteInfo>

    /**
     * Maps a list of page URLs to a list of booleans indicating if each URL was visited.
     *
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
    fun getVisitInfos(
        start: Long,
        end: Long = Long.MAX_VALUE,
        excludeTypes: List<VisitType> = listOf()
    ): List<HistoryVisitInfo>

    /**
     * Return a "page" of history results. Each page will have visits in descending order
     * with respect to their visit timestamps. In the case of ties, their row id will
     * be used.
     *
     * Note that you may get surprising results if the items in the database change
     * while you are paging through records.
     *
     * @param offset The offset where the page begins.
     * @param count The number of items to return in the page.
     * @param excludeTypes List of visit types to exclude.
     */
    fun getVisitPage(offset: Long, count: Long, excludeTypes: List<VisitType> = listOf()): List<HistoryVisitInfo>

    /**
     * Page more efficiently than using simple numeric offset. We first figure out
     * a visited timestamp upper bound, then do a smaller numeric offset relative to
     * the bound.
     *
     * @param bound The upper bound of already visited items.
     * @param offset The offset between first item that has visit date equal to bound
     *  and last visited item.
     * @param count The number eof items to return in the page.
     * @param excludeTypes List of visit types to exclude.
     */
    fun getVisitPageWithBound(
        bound: Long,
        offset: Long,
        count: Long,
        excludeTypes: List<VisitType> = listOf()
    ): HistoryVisitInfosWithBound

    /**
     * Get the number of history visits.
     *
     * It is intended that this be used with `getVisitPage` to allow pagination
     * through records, however be aware that (unless you hold the only
     * reference to the write connection, and know a sync may not occur at this
     * time), the number of items in the database may change between when you
     * call `getVisitCount` and `getVisitPage`.
     *
     *
     * @param excludeTypes List of visit types to exclude.
     */
    fun getVisitCount(excludeTypes: List<VisitType> = listOf()): Long
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
     * Deletes all visits from the given URL. If the page has previously
     * been synced, a tombstone will be written to the Sync server, meaning
     * visits for the page should be deleted from all synced devices. If
     * the page is bookmarked, or has a keyword or tags, only its visits
     * will be removed; otherwise, the page will be removed completely.
     *
     * Note that, if the page is duplicated on the Sync server (that is,
     * the server has a record with the page URL, but its GUID is different
     * than the one we have locally), only the record whose GUID matches the
     * local GUID will be deleted. This is (hopefully) rare, and sadly there
     * is not much we can do about it. It indicates a client-side bug that
     * occurred at some point in the past.
     *
     * @param url the url to be removed.
     */
    fun deleteVisitsFor(url: String)

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
     * Records an accepted autocomplete match, recording the query string,
     * and chosen URL for subsequent matches.
     *
     * @param searchString The query string
     * @param url The chosen URL string
     */
    fun acceptResult(searchString: String, url: String)
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

enum class SearchResultReason {
    KEYWORD,
    ORIGIN,
    URL,
    PREVIOUS_USE,
    BOOKMARK,
    TAG;

    companion object {
        fun fromMessage(reason: MsgTypes.SearchResultReason): SearchResultReason {
            return when (reason) {
                MsgTypes.SearchResultReason.KEYWORD -> KEYWORD
                MsgTypes.SearchResultReason.ORIGIN -> ORIGIN
                MsgTypes.SearchResultReason.URL -> URL
                MsgTypes.SearchResultReason.PREVIOUS_USE -> PREVIOUS_USE
                MsgTypes.SearchResultReason.BOOKMARK -> BOOKMARK
                MsgTypes.SearchResultReason.TAG -> TAG
            }
        }
    }
}

data class SearchResult(
    val url: String,
    val title: String,
    val frecency: Long,
    val reasons: List<SearchResultReason>
) {
    companion object {
        internal fun fromMessage(msg: MsgTypes.SearchResultMessage): SearchResult {
            return SearchResult(
                url = msg.url,
                title = msg.title,
                frecency = msg.frecency,
                reasons = msg.reasonsList.map {
                    SearchResultReason.fromMessage(it)
                }
            )
        }
        internal fun fromCollectionMessage(msg: MsgTypes.SearchResultList): List<SearchResult> {
            return msg.resultsList.map {
                fromMessage(it)
            }
        }
    }
}

/**
 * Represents a set of properties which uniquely identify a history metadata.
 * In database terms this is a compound key.
 * @property url A url of the page.
 * @property searchTerm An optional search term which was used to find this page.
 * @property referrerUrl An optional referrer url for this page.
 */
data class HistoryMetadataKey(
    val url: String,
    val searchTerm: String?,
    val referrerUrl: String?
)

/**
 * A helper class for gathering basic count metrics on different kinds of PlacesManager operations.
 *
 * For each type of operation, we want to measure:
 *    - total count of operations performed
 *    - count of operations that produced an error, labeled by type
 *
 * This is a convenince wrapper to measure the two in one shot.
 */
class PlacesManagerCounterMetrics(
    val count: CounterMetricType,
    val errCount: LabeledMetricType<CounterMetricType>
) {
    @Suppress("ComplexMethod", "TooGenericExceptionCaught")
    inline fun <U> measure(callback: () -> U): U {
        count.add()
        try {
            return callback()
        } catch (e: Exception) {
            when (e) {
                is PlacesException.UrlParseFailed -> {
                    errCount["url_parse_failed"].add()
                }
                is PlacesException.OperationInterrupted -> {
                    errCount["operation_interrupted"].add()
                }
                is PlacesException.InvalidParent -> {
                    errCount["invalid_parent"].add()
                }
                is PlacesException.UnknownBookmarkItem -> {
                    errCount["unknown_bookmark_item"].add()
                }
                is PlacesException.UrlTooLong -> {
                    errCount["url_too_long"].add()
                }
                is PlacesException.InvalidBookmarkUpdate -> {
                    errCount["invalid_bookmark_update"].add()
                }
                is PlacesException.CannotUpdateRoot -> {
                    errCount["cannot_update_root"].add()
                }
                else -> {
                    errCount["__other__"].add()
                }
            }
            throw e
        }
    }
}
