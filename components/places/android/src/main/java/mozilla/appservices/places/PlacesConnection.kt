/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.places

import mozilla.appservices.places.uniffi.BookmarkItem
import mozilla.appservices.places.uniffi.BookmarkPosition
import mozilla.appservices.places.uniffi.BookmarkUpdateInfo
import mozilla.appservices.places.uniffi.ConnectionType
import mozilla.appservices.places.uniffi.DocumentType
import mozilla.appservices.places.uniffi.FrecencyThresholdOption
import mozilla.appservices.places.uniffi.HistoryHighlight
import mozilla.appservices.places.uniffi.HistoryHighlightWeights
import mozilla.appservices.places.uniffi.HistoryMetadata
import mozilla.appservices.places.uniffi.HistoryMetadataObservation
import mozilla.appservices.places.uniffi.HistoryVisitInfo
import mozilla.appservices.places.uniffi.HistoryVisitInfosWithBound
import mozilla.appservices.places.uniffi.InsertableBookmark
import mozilla.appservices.places.uniffi.InsertableBookmarkFolder
import mozilla.appservices.places.uniffi.InsertableBookmarkItem
import mozilla.appservices.places.uniffi.InsertableBookmarkSeparator
import mozilla.appservices.places.uniffi.NoteHistoryMetadataObservationOptions
import mozilla.appservices.places.uniffi.PlacesApiException
import mozilla.appservices.places.uniffi.SearchResult
import mozilla.appservices.places.uniffi.SqlInterruptHandle
import mozilla.appservices.places.uniffi.TopFrecentSiteInfo
import mozilla.appservices.places.uniffi.VisitObservation
import mozilla.appservices.places.uniffi.VisitType
import mozilla.appservices.places.uniffi.placesApiNew
import mozilla.appservices.sync15.SyncTelemetryPing
import mozilla.telemetry.glean.private.CounterMetricType
import mozilla.telemetry.glean.private.LabeledMetricType
import java.lang.ref.WeakReference
import mozilla.appservices.places.uniffi.PlacesApi as UniffiPlacesApi
import mozilla.appservices.places.uniffi.PlacesConnection as UniffiPlacesConnection
import org.mozilla.appservices.places.GleanMetrics.PlacesManager as PlacesManagerMetrics

typealias Url = String
typealias Guid = String

/**
 * An implementation of a [PlacesManager] backed by a Rust Places library.
 *
 * This type, as well as all connection types, are thread safe (they perform locking internally
 * where necessary).
 *
 * @param path an absolute path to a file that will be used for the internal database.
 */
class PlacesApi(path: String) : PlacesManager, AutoCloseable {
    // References to our "api" object and the single writer connection.
    private var api: UniffiPlacesApi
    private var writeConn: PlacesWriterConnection

    init {
        // as per https://github.com/mozilla/uniffi-rs/pull/1063, there was some
        // pushback on allowing this to actually be a constructor, so it's a global
        // function instead :(
        api = placesApiNew(path)

        val uniffiConnection = api.newConnection(ConnectionType.READ_WRITE)
        writeConn = PlacesWriterConnection(uniffiConnection, this)
    }

    companion object {
        // These numbers come from `places::db::ConnectionType`
        private const val READ_ONLY: Int = 1
        private const val READ_WRITE: Int = 2
    }

    override fun registerWithSyncManager() {
        this.api.registerWithSyncManager()
    }

    override fun openReader(): PlacesReaderConnection {
        val conn = api.newConnection(ConnectionType.READ_ONLY)
        return PlacesReaderConnection(conn)
    }

    override fun getWriter(): PlacesWriterConnection {
        return writeConn
    }

    @Synchronized
    override fun close() {
        this.writeConn.apiRef.clear()
    }
}

@Suppress("TooGenericExceptionCaught")
open class PlacesConnection internal constructor(uniffiConn: UniffiPlacesConnection) :
    InterruptibleConnection, AutoCloseable {
    protected var conn: UniffiPlacesConnection
    protected var interruptHandle: SqlInterruptHandle

    init {
        interruptHandle = uniffiConn.newInterruptHandle()
        conn = uniffiConn
    }

    @Synchronized
    protected fun destroy() {
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
}

/**
 * An implementation of a [ReadableHistoryConnection], used for read-only
 * access to places APIs.
 *
 * This class is thread safe.
 */
open class PlacesReaderConnection internal constructor(conn: UniffiPlacesConnection) :
    PlacesConnection(conn),
    ReadableHistoryConnection,
    ReadableHistoryMetadataConnection,
    ReadableBookmarksConnection {
    override fun queryAutocomplete(query: String, limit: Int): List<SearchResult> {
        return this.conn.queryAutocomplete(query, limit)
    }

    override fun matchUrl(query: String): Url? {
        return this.conn.matchUrl(query)
    }

    override fun getTopFrecentSiteInfos(
        numItems: Int,
        frecencyThreshold: FrecencyThresholdOption,
    ): List<TopFrecentSiteInfo> {
        return this.conn.getTopFrecentSiteInfos(numItems, frecencyThreshold)
    }

    override fun getVisited(urls: List<String>): List<Boolean> {
        return this.conn.getVisited(urls)
    }

    override fun getVisitedUrlsInRange(start: Long, end: Long, includeRemote: Boolean): List<String> {
        return this.conn.getVisitedUrlsInRange(start, end, includeRemote)
    }

    override fun getVisitInfos(start: Long, end: Long, excludeTypes: List<VisitType>): List<HistoryVisitInfo> {
        return this.conn.getVisitInfos(start, end, visitTransitionSet(excludeTypes))
    }

    override fun getVisitPage(offset: Long, count: Long, excludeTypes: List<VisitType>): List<HistoryVisitInfo> {
        return this.conn.getVisitPage(offset, count, visitTransitionSet(excludeTypes))
    }

    override fun getVisitPageWithBound(
        bound: Long,
        offset: Long,
        count: Long,
        excludeTypes: List<VisitType>,
    ): HistoryVisitInfosWithBound {
        return this.conn.getVisitPageWithBound(offset, bound, count, visitTransitionSet(excludeTypes))
    }

    override fun getVisitCount(excludeTypes: List<VisitType>): Long {
        return this.conn.getVisitCount(visitTransitionSet(excludeTypes))
    }

    override suspend fun getLatestHistoryMetadataForUrl(url: Url): HistoryMetadata? {
        return this.conn.getLatestHistoryMetadataForUrl(url)
    }

    override suspend fun getHistoryMetadataSince(since: Long): List<HistoryMetadata> {
        return this.conn.getHistoryMetadataSince(since)
    }

    override suspend fun getHistoryMetadataBetween(start: Long, end: Long): List<HistoryMetadata> {
        return this.conn.getHistoryMetadataBetween(start, end)
    }

    override suspend fun queryHistoryMetadata(query: String, limit: Int): List<HistoryMetadata> {
        return this.conn.queryHistoryMetadata(query, limit)
    }

    override suspend fun getHighlights(
        weights: HistoryHighlightWeights,
        limit: Int,
    ): List<HistoryHighlight> {
        return this.conn.getHistoryHighlights(weights, limit)
    }

    override fun getBookmark(guid: Guid): BookmarkItem? {
        return this.conn.bookmarksGetByGuid(guid, false)
    }

    override fun getBookmarksTree(rootGUID: Guid, recursive: Boolean): BookmarkItem? {
        if (recursive) {
            return this.conn.bookmarksGetTree(rootGUID)
        } else {
            return this.conn.bookmarksGetByGuid(rootGUID, true)
        }
    }

    override fun getBookmarksWithURL(url: Url): List<BookmarkItem> {
        return this.conn.bookmarksGetAllWithUrl(url)
    }

    override fun getBookmarkUrlForKeyword(keyword: String): Url? {
        return this.conn.bookmarksGetUrlForKeyword(keyword)
    }

    override fun searchBookmarks(query: String, limit: Int): List<BookmarkItem> {
        return this.conn.bookmarksSearch(query, limit)
    }

    override fun getRecentBookmarks(limit: Int): List<BookmarkItem> {
        return this.conn.bookmarksGetRecent(limit)
    }

    override fun countBookmarksInTrees(guids: List<Guid>): UInt {
        return this.conn.bookmarksCountBookmarksInTrees(guids)
    }
}

@Suppress("MagicNumber")
internal fun VisitType.toInt(): Int {
    return when (this) {
        VisitType.LINK -> 1
        VisitType.TYPED -> 2
        VisitType.BOOKMARK -> 3
        VisitType.EMBED -> 4
        VisitType.REDIRECT_PERMANENT -> 5
        VisitType.REDIRECT_TEMPORARY -> 6
        VisitType.DOWNLOAD -> 7
        VisitType.FRAMED_LINK -> 8
        VisitType.RELOAD -> 9
        VisitType.UPDATE_PLACE -> 10
    }
}

fun visitTransitionSet(l: List<VisitType>): Int {
    var res = 0
    for (ty in l) {
        res = res or (1 shl ty.toInt())
    }
    return res
}

/**
 * An implementation of a [WritableHistoryConnection], use for read or write
 * access to the Places APIs.
 *
 * This class is thread safe.
 */
class PlacesWriterConnection internal constructor(conn: UniffiPlacesConnection, api: PlacesApi) :
    PlacesReaderConnection(conn),
    WritableHistoryConnection,
    WritableHistoryMetadataConnection,
    WritableBookmarksConnection {
    // The reference to our PlacesAPI. Mostly used to know how to handle getting closed.
    val apiRef = WeakReference(api)
    override fun noteObservation(data: VisitObservation) {
        this.conn.applyObservation(data)
    }

    override fun deleteVisitsFor(url: String) {
        return this.conn.deleteVisitsFor(url)
    }

    override fun deleteVisit(url: String, visitTimestamp: Long) {
        return this.conn.deleteVisit(url, visitTimestamp)
    }

    override fun deleteVisitsSince(since: Long) {
        deleteVisitsBetween(since, Long.MAX_VALUE)
    }

    override fun deleteVisitsBetween(startTime: Long, endTime: Long) {
        return this.conn.deleteVisitsBetween(startTime, endTime)
    }

    @Suppress("MagicNumber")
    override fun runMaintenance(dbSizeLimit: UInt) {
        val pruneMetrics = PlacesManagerMetrics.runMaintenanceTime.measure {
            val pruneMetrics = PlacesManagerMetrics.runMaintenancePruneTime.measure {
                this.conn.runMaintenancePrune(dbSizeLimit, 12U)
            }

            PlacesManagerMetrics.runMaintenanceVacuumTime.measure {
                this.conn.runMaintenanceVacuum()
            }

            PlacesManagerMetrics.runMaintenanceOptimizeTime.measure {
                this.conn.runMaintenanceOptimize()
            }

            PlacesManagerMetrics.runMaintenanceChkPntTime.measure {
                this.conn.runMaintenanceCheckpoint()
            }
            pruneMetrics
        }
        PlacesManagerMetrics.dbSizeAfterMaintenance.accumulateSamples(listOf(pruneMetrics.dbSizeAfter.toLong() / 1024))
    }

    override fun deleteEverything() {
        return this.conn.deleteEverythingHistory()
    }

    override fun deleteAllBookmarks() {
        return this.conn.bookmarksDeleteEverything()
    }

    override fun deleteBookmarkNode(guid: Guid): Boolean {
        return this.conn.bookmarksDelete(guid)
    }

    override suspend fun noteHistoryMetadataObservation(
        observation: HistoryMetadataObservation,
        options: NoteHistoryMetadataObservationOptions,
    ) {
        // Different types of `HistoryMetadataObservation` are flattened out into a list of values.
        // The other side of this (rust code) is going to deal with missing/absent values. We're just
        // passing them along here.
        // NB: Even though `MsgTypes.HistoryMetadataObservation` has an optional title field, we ignore it here.
        // That's used by consumers which aren't already using the history observation APIs.
        return this.conn.noteHistoryMetadataObservation(observation, options)
    }

    override suspend fun deleteHistoryMetadataOlderThan(olderThan: Long) {
        this.conn.metadataDeleteOlderThan(olderThan)
    }

    override suspend fun deleteHistoryMetadata(key: HistoryMetadataKey) {
        this.conn.metadataDelete(
            key.url,
            key.referrerUrl,
            key.searchTerm,
        )
    }

    // Does the shared insert work.
    private fun doInsert(item: InsertableBookmarkItem): Guid {
        return this.conn.bookmarksInsert(item)
    }

    override fun createFolder(parentGUID: Guid, title: String, position: UInt?): Guid {
        val p = if (position == null) {
            BookmarkPosition.Append
        } else {
            BookmarkPosition.Specific(position)
        }
        val folder = InsertableBookmarkFolder(
            parentGuid = parentGUID,
            position = p,
            title = title,
            children = emptyList(),
        )
        return this.doInsert(InsertableBookmarkItem.Folder(folder))
    }

    override fun createSeparator(parentGUID: Guid, position: UInt?): Guid {
        val p = if (position == null) {
            BookmarkPosition.Append
        } else {
            BookmarkPosition.Specific(position)
        }
        val sep = InsertableBookmarkSeparator(parentGuid = parentGUID, position = p)
        return this.doInsert(InsertableBookmarkItem.Separator(sep))
    }

    override fun createBookmarkItem(parentGUID: Guid, url: Url, title: String, position: UInt?): Guid {
        val p = if (position == null) {
            BookmarkPosition.Append
        } else {
            BookmarkPosition.Specific(position)
        }
        val bm = InsertableBookmark(parentGuid = parentGUID, position = p, url = url, title = title)
        return this.doInsert(InsertableBookmarkItem.Bookmark(bm))
    }

    override fun updateBookmark(guid: Guid, parentGuid: Guid?, position: UInt?, title: String?, url: Url?) {
        val p: UInt? = if (position == null) {
            null
        } else {
            position
        }
        val info = BookmarkUpdateInfo(guid = guid, title = title, url = url, parentGuid = parentGuid, position = p)
        return this.conn.bookmarksUpdate(info)
    }

    override fun acceptResult(searchString: String, url: String) {
        return this.conn.acceptResult(searchString, url)
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
    val tokenserverURL: String,
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
     * @param since Timestamp to search by.
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
    suspend fun noteHistoryMetadataObservation(
        observation: HistoryMetadataObservation,
        options: NoteHistoryMetadataObservationOptions = NoteHistoryMetadataObservationOptions(),
    )

    // There's a bit of an impedance mismatch here; `HistoryMetadataKey` is
    // a concept that only exists here and not in the rust. We can iterate on
    // this as the entire "history metadata" requirement evolves.
    suspend fun noteHistoryMetadataObservationViewTime(
        key: HistoryMetadataKey,
        viewTime: Int,
        options: NoteHistoryMetadataObservationOptions = NoteHistoryMetadataObservationOptions(),
    ) {
        val obs = HistoryMetadataObservation(
            url = key.url,
            searchTerm = key.searchTerm,
            referrerUrl = key.referrerUrl,
            viewTime = viewTime,
        )
        noteHistoryMetadataObservation(obs, options)
    }

    suspend fun noteHistoryMetadataObservationDocumentType(
        key: HistoryMetadataKey,
        documentType: DocumentType,
        options: NoteHistoryMetadataObservationOptions = NoteHistoryMetadataObservationOptions(),
    ) {
        val obs = HistoryMetadataObservation(
            url = key.url,
            searchTerm = key.searchTerm,
            referrerUrl = key.referrerUrl,
            documentType = documentType,
        )
        noteHistoryMetadataObservation(obs, options)
    }

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
        excludeTypes: List<VisitType> = listOf(),
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
        excludeTypes: List<VisitType> = listOf(),
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
     * Run periodic database maintenance. This might include, but is not limited
     * to:
     *
     * - `VACUUM`ing.
     * - Requesting that the indices in our tables be optimized.
     * - Expiring irrelevant history visits.
     * - Periodic repair or deletion of corrupted records.
     * - Deleting older visits when the database exceeds dbSizeLimit
     * - etc.
     *
     * Maintenance in performed in small chunks at a time to avoid blocking the
     * DB connection for too long.  This means that this should be called
     * regularly when the app is idle.
     *
     * @param dbSizeLimit: Maximum DB size to aim for, in bytes.  If the
     * database exceeds this size, we will prune a small number of visits.
     * For reference, desktop normally uses 75 MiB (78643200).  If it
     * determines that either the disk or memory is constrained then it halves
     * the amount. The default of 0 disables pruning.
     */
    fun runMaintenance(dbSizeLimit: UInt = 0U)

    /**
     * Delete everything locally.
     *
     * This will not delete visits from remote devices, however it will
     * prevent them from trickling in over time when future syncs occur.
     *
     * The difference between this and wipeLocal is that wipeLocal does
     * not prevent the deleted visits from returning. For wipeLocal,
     * the visits will return on the next full sync (which may be
     * arbitrarially far in the future), whereas items which were
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
    val referrerUrl: String?,
)

/**
 * A helper class for gathering basic count metrics on different kinds of PlacesManager operations.
 *
 * For each type of operation, we want to measure:
 *    - total count of operations performed
 *    - count of operations that produced an error, labeled by type
 *
 * This is a convenience wrapper to measure the two in one shot.
 */
class PlacesManagerCounterMetrics(
    val count: CounterMetricType,
    val errCount: LabeledMetricType<CounterMetricType>,
) {
    @Suppress("ComplexMethod", "TooGenericExceptionCaught")
    inline fun <U> measure(callback: () -> U): U {
        count.add()
        try {
            return callback()
        } catch (e: Exception) {
            when (e) {
                is PlacesApiException.UrlParseFailed -> {
                    errCount["url_parse_failed"].add()
                }
                is PlacesApiException.OperationInterrupted -> {
                    errCount["operation_interrupted"].add()
                }
                is PlacesApiException.UnknownBookmarkItem -> {
                    errCount["unknown_bookmark_item"].add()
                }
                is PlacesApiException.InvalidBookmarkOperation -> {
                    errCount["invalid_bookmark_operation"].add()
                }
                is PlacesApiException.PlacesConnectionBusy -> {
                    errCount["places_connection_busy"].add()
                }
                is PlacesApiException.UnexpectedPlacesException -> {
                    errCount["unexpected_places_exception"].add()
                }
                else -> {
                    errCount["__other__"].add()
                }
            }
            throw e
        }
    }
}
