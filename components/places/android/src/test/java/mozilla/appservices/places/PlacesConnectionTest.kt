/* Any copyright is dedicated to the Public Domain.
   http://creativecommons.org/publicdomain/zero/1.0/ */

package mozilla.appservices.places

import androidx.test.core.app.ApplicationProvider
import kotlinx.coroutines.runBlocking
import mozilla.appservices.places.uniffi.BookmarkItem
import mozilla.appservices.places.uniffi.DocumentType
import mozilla.appservices.places.uniffi.FrecencyThresholdOption
import mozilla.appservices.places.uniffi.HistoryMetadataPageMissingBehavior
import mozilla.appservices.places.uniffi.NoteHistoryMetadataObservationOptions
import mozilla.appservices.places.uniffi.PlacesApiException
import mozilla.appservices.places.uniffi.VisitObservation
import mozilla.appservices.places.uniffi.VisitType
import mozilla.appservices.syncmanager.SyncManager
import mozilla.telemetry.glean.testing.GleanTestRule
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.mozilla.appservices.places.GleanMetrics.PlacesManager as PlacesManagerMetrics

@RunWith(RobolectricTestRunner::class)
@Config(manifest = Config.NONE)
class PlacesConnectionTest {
    @Rule
    @JvmField
    val dbFolder = TemporaryFolder()

    @get:Rule
    val gleanRule = GleanTestRule(ApplicationProvider.getApplicationContext())

    lateinit var api: PlacesApi
    lateinit var db: PlacesWriterConnection

    @Before
    fun initAPI() {
        api = PlacesApi(path = dbFolder.newFile().absolutePath)
        db = api.getWriter()
    }

    @After
    fun closeAPI() {
        db.close()
        api.close()
    }

    // Basically equivalent to test_get_visited in rust, but exercises the FFI,
    // as well as the handling of invalid urls.
    @Test
    fun testGetVisited() {
        val unicodeInPath = "http://www.example.com/tëst😀abc"
        val escapedUnicodeInPath = "http://www.example.com/t%C3%ABst%F0%9F%98%80abc"

        val unicodeInDomain = "http://www.exämple😀123.com"
        val escapedUnicodeInDomain = "http://www.xn--exmple123-w2a24222l.com"

        val toAdd = listOf(
            "https://www.example.com/1",
            "https://www.example.com/12",
            "https://www.example.com/123",
            "https://www.example.com/1234",
            "https://www.mozilla.com",
            "https://www.firefox.com",
            "$unicodeInPath/1",
            "$escapedUnicodeInPath/2",
            "$unicodeInDomain/1",
            "$escapedUnicodeInDomain/2",
        )

        for (url in toAdd) {
            db.noteObservation(VisitObservation(url = url, visitType = VisitType.LINK))
        }

        val toSearch = listOf(
            Pair("https://www.example.com", false),
            Pair("https://www.example.com/1", true),
            Pair("https://www.example.com/12", true),
            Pair("https://www.example.com/123", true),
            Pair("https://www.example.com/1234", true),
            Pair("https://www.example.com/12345", false),
            // Bad URLs should still work without.
            Pair("https://www.example.com:badurl", false),

            Pair("https://www.mozilla.com", true),
            Pair("https://www.firefox.com", true),
            Pair("https://www.mozilla.org", false),

            // Dupes should still work
            Pair("https://www.example.com/1234", true),
            Pair("https://www.example.com/12345", false),

            // The unicode URLs should work when escaped the way we
            // encountered them
            Pair("$unicodeInPath/1", true),
            Pair("$escapedUnicodeInPath/2", true),
            Pair("$unicodeInDomain/1", true),
            Pair("$escapedUnicodeInDomain/2", true),

            // But also the other way.
            Pair("$unicodeInPath/2", true),
            Pair("$escapedUnicodeInPath/1", true),
            Pair("$unicodeInDomain/2", true),
            Pair("$escapedUnicodeInDomain/1", true),
        )

        val visited = db.getVisited(toSearch.map { it.first }.toList())

        assertEquals(visited.size, toSearch.size)

        for (i in 0 until visited.size) {
            assert(visited[i] == toSearch[i].second) {
                "Test $i failed for url ${toSearch[i].first} (got ${visited[i]}, want ${toSearch[i].second})"
            }
        }
    }

    @Test
    fun testNoteObservationBadUrl() {
        try {
            db.noteObservation(VisitObservation(url = "http://www.[].com", visitType = VisitType.LINK))
        } catch (e: PlacesApiException) {
            assert(e is PlacesApiException.UrlParseFailed)
        }
    }

    // Basically equivalent to test_get_visited in rust, but exercises the FFI,
    // as well as the handling of invalid urls.
    @Test
    fun testMatchUrl() {
        val toAdd = listOf(
            // add twice to ensure its frecency is higher
            "https://www.example.com/123",
            "https://www.example.com/123",
            "https://www.example.com/12345",
            "https://www.mozilla.com/foo/bar/baz",
            "https://www.mozilla.com/foo/bar/baz",
            "https://mozilla.com/a1/b2/c3",
            "https://news.ycombinator.com/",
        )

        for (url in toAdd) {
            db.noteObservation(VisitObservation(url = url, visitType = VisitType.LINK))
        }
        // Should use the origin search
        assertEquals("https://www.example.com/", db.matchUrl("example.com"))
        assertEquals("https://www.example.com/", db.matchUrl("www.example.com"))
        assertEquals("https://www.example.com/", db.matchUrl("https://www.example.com"))

        // Not an origin.
        assertEquals("https://www.example.com/123", db.matchUrl("example.com/"))
        assertEquals("https://www.example.com/123", db.matchUrl("www.example.com/"))
        assertEquals("https://www.example.com/123", db.matchUrl("https://www.example.com/"))

        assertEquals("https://www.example.com/123", db.matchUrl("example.com/1"))
        assertEquals("https://www.example.com/123", db.matchUrl("www.example.com/1"))
        assertEquals("https://www.example.com/123", db.matchUrl("https://www.example.com/1"))

        assertEquals("https://www.example.com/12345", db.matchUrl("example.com/1234"))
        assertEquals("https://www.example.com/12345", db.matchUrl("www.example.com/1234"))
        assertEquals("https://www.example.com/12345", db.matchUrl("https://www.example.com/1234"))

        assertEquals("https://www.mozilla.com/foo/", db.matchUrl("mozilla.com/"))
        assertEquals("https://www.mozilla.com/foo/", db.matchUrl("mozilla.com/foo"))
        assertEquals("https://www.mozilla.com/foo/bar/", db.matchUrl("mozilla.com/foo/"))
        assertEquals("https://www.mozilla.com/foo/bar/", db.matchUrl("mozilla.com/foo/bar"))
        assertEquals("https://www.mozilla.com/foo/bar/baz", db.matchUrl("mozilla.com/foo/bar/"))
        assertEquals("https://www.mozilla.com/foo/bar/baz", db.matchUrl("mozilla.com/foo/bar/baz"))
        // Make sure the www/non-www doesn't confuse it
        assertEquals("https://mozilla.com/a1/b2/", db.matchUrl("mozilla.com/a1/"))

        // Actual visit had no www
        assertEquals(null, db.matchUrl("www.mozilla.com/a1"))
        assertEquals("https://news.ycombinator.com/", db.matchUrl("news"))
    }

    @Test
    fun testObservingPreviewImage() {
        db.noteObservation(
            VisitObservation(
                url = "https://www.example.com/0",
                visitType = VisitType.LINK,
            ),
        )

        db.noteObservation(
            VisitObservation(
                url = "https://www.example.com/1",
                visitType = VisitType.LINK,
            ),
        )

        // Can change preview image.
        db.noteObservation(
            VisitObservation(
                url = "https://www.example.com/1",
                visitType = VisitType.LINK,
                previewImageUrl = "https://www.example.com/1/previewImage.png",
            ),
        )

        // Can make an initial observation with the preview image.
        db.noteObservation(
            VisitObservation(
                url = "https://www.example.com/2",
                visitType = VisitType.LINK,
                previewImageUrl = "https://www.example.com/2/previewImage.png",
            ),
        )

        val all = db.getVisitInfos(0)
        assertEquals(4, all.count())
        assertNull(all[0].previewImageUrl)
        assertEquals("https://www.example.com/1/previewImage.png", all[1].previewImageUrl)
        assertEquals("https://www.example.com/1/previewImage.png", all[2].previewImageUrl)
        assertEquals("https://www.example.com/2/previewImage.png", all[3].previewImageUrl)
    }

    @Test
    fun testGetTopFrecentSiteInfos() {
        db.noteObservation(VisitObservation(url = "https://www.example.com/1", visitType = VisitType.DOWNLOAD))
        db.noteObservation(VisitObservation(url = "https://www.example.com/1", visitType = VisitType.EMBED))
        db.noteObservation(VisitObservation(url = "https://www.example.com/1", visitType = VisitType.REDIRECT_PERMANENT))
        db.noteObservation(VisitObservation(url = "https://www.example.com/1", visitType = VisitType.REDIRECT_TEMPORARY))
        db.noteObservation(VisitObservation(url = "https://www.example.com/1", visitType = VisitType.FRAMED_LINK))
        db.noteObservation(VisitObservation(url = "https://www.example.com/1", visitType = VisitType.RELOAD))

        val toAdd = listOf(
            "https://www.example.com/123",
            "https://www.example.com/123",
            "https://www.example.com/12345",
            "https://www.mozilla.com/foo/bar/baz",
            "https://www.mozilla.com/foo/bar/baz",
            "https://mozilla.com/a1/b2/c3",
            "https://news.ycombinator.com/",
            "https://www.mozilla.com/foo/bar/baz",
        )

        for (url in toAdd) {
            db.noteObservation(VisitObservation(url = url, visitType = VisitType.LINK))
        }

        var infos = db.getTopFrecentSiteInfos(numItems = 0, frecencyThreshold = FrecencyThresholdOption.NONE)

        assertEquals(0, infos.size)

        infos = db.getTopFrecentSiteInfos(numItems = 0, frecencyThreshold = FrecencyThresholdOption.SKIP_ONE_TIME_PAGES)

        assertEquals(0, infos.size)

        infos = db.getTopFrecentSiteInfos(numItems = 3, frecencyThreshold = FrecencyThresholdOption.NONE)

        assertEquals(3, infos.size)
        assertEquals("https://www.mozilla.com/foo/bar/baz", infos[0].url)
        assertEquals("https://www.example.com/123", infos[1].url)
        assertEquals("https://news.ycombinator.com/", infos[2].url)

        infos = db.getTopFrecentSiteInfos(numItems = 3, frecencyThreshold = FrecencyThresholdOption.SKIP_ONE_TIME_PAGES)

        assertEquals(2, infos.size)
        assertEquals("https://www.mozilla.com/foo/bar/baz", infos[0].url)
        assertEquals("https://www.example.com/123", infos[1].url)

        infos = db.getTopFrecentSiteInfos(numItems = 5, frecencyThreshold = FrecencyThresholdOption.NONE)

        assertEquals(5, infos.size)
        assertEquals("https://www.mozilla.com/foo/bar/baz", infos[0].url)
        assertEquals("https://www.example.com/123", infos[1].url)
        assertEquals("https://news.ycombinator.com/", infos[2].url)
        assertEquals("https://mozilla.com/a1/b2/c3", infos[3].url)
        assertEquals("https://www.example.com/12345", infos[4].url)

        infos = db.getTopFrecentSiteInfos(numItems = 5, frecencyThreshold = FrecencyThresholdOption.SKIP_ONE_TIME_PAGES)

        assertEquals(2, infos.size)
        assertEquals("https://www.mozilla.com/foo/bar/baz", infos[0].url)
        assertEquals("https://www.example.com/123", infos[1].url)
    }

    // Basically equivalent to test_get_visited in rust, but exercises the FFI,
    // as well as the handling of invalid urls.
    @Test
    fun testGetVisitInfos() {
        db.noteObservation(VisitObservation(url = "https://www.example.com/1", visitType = VisitType.LINK, at = 100000))
        db.noteObservation(VisitObservation(url = "https://www.example.com/2a", visitType = VisitType.REDIRECT_TEMPORARY, at = 130000))
        db.noteObservation(VisitObservation(url = "https://www.example.com/2b", visitType = VisitType.LINK, at = 150000))
        db.noteObservation(VisitObservation(url = "https://www.example.com/3", visitType = VisitType.LINK, at = 200000))
        db.noteObservation(VisitObservation(url = "https://www.example.com/4", visitType = VisitType.LINK, at = 250000))
        var infos = db.getVisitInfos(125000, 225000, excludeTypes = listOf(VisitType.REDIRECT_TEMPORARY))
        assertEquals(2, infos.size)
        assertEquals("https://www.example.com/2b", infos[0].url)
        assertEquals("https://www.example.com/3", infos[1].url)
        infos = db.getVisitInfos(125000, 225000)
        assertEquals(3, infos.size)
        assertEquals("https://www.example.com/2a", infos[0].url)
        assertEquals("https://www.example.com/2b", infos[1].url)
        assertEquals("https://www.example.com/3", infos[2].url)
    }

    @Test
    fun testGetVisitPage() {
        db.noteObservation(VisitObservation(url = "https://www.example.com/1", visitType = VisitType.LINK, at = 100000))
        db.noteObservation(VisitObservation(url = "https://www.example.com/2", visitType = VisitType.LINK, at = 110000))
        db.noteObservation(VisitObservation(url = "https://www.example.com/3a", visitType = VisitType.REDIRECT_TEMPORARY, at = 120000))
        db.noteObservation(VisitObservation(url = "https://www.example.com/3b", visitType = VisitType.REDIRECT_TEMPORARY, at = 130000))
        db.noteObservation(VisitObservation(url = "https://www.example.com/4", visitType = VisitType.LINK, at = 140000))
        db.noteObservation(VisitObservation(url = "https://www.example.com/5", visitType = VisitType.LINK, at = 150000))
        db.noteObservation(VisitObservation(url = "https://www.example.com/6", visitType = VisitType.LINK, at = 160000))
        db.noteObservation(VisitObservation(url = "https://www.example.com/7", visitType = VisitType.LINK, at = 170000))
        db.noteObservation(VisitObservation(url = "https://www.example.com/8", visitType = VisitType.LINK, at = 180000))

        assertEquals(9, db.getVisitCount())
        assertEquals(7, db.getVisitCount(excludeTypes = listOf(VisitType.REDIRECT_TEMPORARY)))

        val want = listOf(
            listOf("https://www.example.com/8", "https://www.example.com/7", "https://www.example.com/6"),
            listOf("https://www.example.com/5", "https://www.example.com/4", "https://www.example.com/2"),
            listOf("https://www.example.com/1"),
        )

        var offset = 0L
        for (expect in want) {
            val page = db.getVisitPage(
                offset = offset,
                count = 3,
                excludeTypes = listOf(VisitType.REDIRECT_TEMPORARY),
            )
            assertEquals(expect.size, page.size)
            for (i in 0..(expect.size - 1)) {
                assertEquals(expect[i], page[i].url)
            }
            offset += page.size
        }
        val empty = db.getVisitPage(
            offset = offset,
            count = 3,
            excludeTypes = listOf(VisitType.REDIRECT_TEMPORARY),
        )
        assertEquals(0, empty.size)
    }

    @Test
    fun testCreateBookmark() {
        val itemGUID = db.createBookmarkItem(
            parentGUID = BookmarkRoot.Unfiled.id,
            url = "https://www.example.com/",
            title = "example",
        )

        val sepGUID = db.createSeparator(
            parentGUID = BookmarkRoot.Unfiled.id,
            position = 0u,
        )

        val folderGUID = db.createFolder(
            parentGUID = BookmarkRoot.Unfiled.id,
            title = "example folder",
        )

        val item = db.getBookmark(itemGUID)!! as BookmarkItem.Bookmark
        val sep = db.getBookmark(sepGUID)!! as BookmarkItem.Separator
        val folder = db.getBookmark(folderGUID)!! as BookmarkItem.Folder

        assertEquals(item.b.title, "example")
        assertEquals(item.b.url, "https://www.example.com/")
        assertEquals(item.b.position, 1u)
        assertEquals(item.b.parentGuid, BookmarkRoot.Unfiled.id)

        assertEquals(sep.s.position, 0u)
        assertEquals(sep.s.parentGuid, BookmarkRoot.Unfiled.id)

        assertEquals(folder.f.title, "example folder")
        assertEquals(folder.f.position, 2u)
        assertEquals(folder.f.parentGuid, BookmarkRoot.Unfiled.id)
    }

    @Test
    fun testCountBookmarks() {
        assertEquals(db.countBookmarksInTrees(listOf(BookmarkRoot.Unfiled.id)), 0U)

        db.createBookmarkItem(
            parentGUID = BookmarkRoot.Unfiled.id,
            url = "https://www.example.com/",
            title = "example",
        )
        assertEquals(db.countBookmarksInTrees(listOf(BookmarkRoot.Unfiled.id)), 1U)

        val folderGUID = db.createFolder(
            parentGUID = BookmarkRoot.Unfiled.id,
            title = "example folder",
        )
        // Folders don't get counted.
        assertEquals(db.countBookmarksInTrees(listOf(BookmarkRoot.Unfiled.id)), 1U)

        // new item in the child folder does.
        db.createBookmarkItem(
            parentGUID = folderGUID,
            url = "https://www.example.com/",
            title = "example",
        )
        assertEquals(db.countBookmarksInTrees(listOf(BookmarkRoot.Unfiled.id)), 2U)

        // A separator is not counted.
        db.createSeparator(
            parentGUID = BookmarkRoot.Unfiled.id,
            position = 0u,
        )
        assertEquals(db.countBookmarksInTrees(listOf(BookmarkRoot.Unfiled.id)), 2U)
    }

    @Test
    fun testHistoryMetricsGathering() {
        assertNull(PlacesManagerMetrics.writeQueryCount.testGetValue())
        assertNull(PlacesManagerMetrics.writeQueryErrorCount["url_parse_failed"].testGetValue())

        db.noteObservation(VisitObservation(url = "https://www.example.com/2a", visitType = VisitType.REDIRECT_TEMPORARY, at = 130000))
        db.noteObservation(VisitObservation(url = "https://www.example.com/2b", visitType = VisitType.LINK, at = 150000))
        db.noteObservation(VisitObservation(url = "https://www.example.com/3", visitType = VisitType.LINK, at = 200000))

        assertEquals(3, PlacesManagerMetrics.writeQueryCount.testGetValue())
        assertNull(PlacesManagerMetrics.writeQueryErrorCount["__other__"].testGetValue())

        try {
            db.noteObservation(VisitObservation(url = "4", visitType = VisitType.REDIRECT_TEMPORARY, at = 160000))
            fail("Should have thrown")
        } catch (e: PlacesApiException.UrlParseFailed) {
            // nothing to do here
        }

        assertEquals(4, PlacesManagerMetrics.writeQueryCount.testGetValue())
        assertEquals(1, PlacesManagerMetrics.writeQueryErrorCount["url_parse_failed"].testGetValue())

        assertNull(PlacesManagerMetrics.readQueryCount.testGetValue())
        assertNull(PlacesManagerMetrics.readQueryErrorCount["__other__"].testGetValue())

        db.getVisitInfos(125000, 225000)

        assertEquals(1, PlacesManagerMetrics.readQueryCount.testGetValue())
        assertNull(PlacesManagerMetrics.readQueryErrorCount["__other__"].testGetValue())

        db.deleteVisit("https://www.example.com/2a", 130000)

        val infos = db.getVisitInfos(130000, 200000)
        assertEquals(2, infos.size)

        assertEquals(5, PlacesManagerMetrics.writeQueryCount.testGetValue())
        assertNull(PlacesManagerMetrics.writeQueryErrorCount["_other_"].testGetValue())
    }

    @Test
    fun testBookmarksMetricsGathering() {
        assertNull(PlacesManagerMetrics.writeQueryCount.testGetValue())
        assertNull(PlacesManagerMetrics.writeQueryErrorCount["unknown_bookmark_item"].testGetValue())

        val itemGUID = db.createBookmarkItem(
            parentGUID = BookmarkRoot.Unfiled.id,
            url = "https://www.example.com/",
            title = "example",
        )

        assertEquals(1, PlacesManagerMetrics.writeQueryCount.testGetValue())
        assertNull(PlacesManagerMetrics.writeQueryErrorCount["unknown_bookmark_item"].testGetValue())

        try {
            db.createBookmarkItem(
                parentGUID = BookmarkRoot.Unfiled.id,
                url = "3",
                title = "example",
            )
            fail("Should have thrown")
        } catch (e: PlacesApiException.UrlParseFailed) {
            // nothing to do here
        }

        assertEquals(2, PlacesManagerMetrics.writeQueryCount.testGetValue())
        assertEquals(1, PlacesManagerMetrics.writeQueryErrorCount["url_parse_failed"].testGetValue())

        assertNull(PlacesManagerMetrics.readQueryCount.testGetValue())
        assertNull(PlacesManagerMetrics.readQueryErrorCount["__other__"].testGetValue())

        db.getBookmark(itemGUID)

        assertEquals(1, PlacesManagerMetrics.readQueryCount.testGetValue())
        assertNull(PlacesManagerMetrics.readQueryErrorCount["__other__"].testGetValue())

        val folderGUID = db.createFolder(
            parentGUID = BookmarkRoot.Unfiled.id,
            title = "example folder",
        )

        db.createBookmarkItem(
            parentGUID = folderGUID,
            url = "https://www.example2.com/",
            title = "example2",
        )

        db.createBookmarkItem(
            parentGUID = folderGUID,
            url = "https://www.example3.com/",
            title = "example3",
        )

        db.createBookmarkItem(
            parentGUID = BookmarkRoot.Unfiled.id,
            url = "https://www.example4.com/",
            title = "example4",
        )

        db.getBookmarksTree(folderGUID, false)
    }

    @Test
    fun testHistoryMetadataBasics() = runBlocking {
        val currentTime = System.currentTimeMillis()

        assertEquals(0, db.getHistoryMetadataSince(0L).size)
        assertEquals(0, db.queryHistoryMetadata("test", 100).size)
        db.noteObservation(
            VisitObservation(
                url = "https://www.ifixit.com/News/35377/which-wireless-earbuds-are-the-least-evil",
                title = "Are All Wireless Earbuds As Evil As AirPods?",
                previewImageUrl = "https://valkyrie.cdn.ifixit.com/media/2020/02/03121341/bose_soundsport_13.jpg",
                visitType = VisitType.LINK,
            ),
        )

        val metaKey1 = HistoryMetadataKey(
            url = "https://www.ifixit.com/News/35377/which-wireless-earbuds-are-the-least-evil",
            searchTerm = "repairable wireless headset",
            referrerUrl = "https://www.google.com/search?client=firefox-b-d&q=headsets+ifixit",
        )

        db.noteHistoryMetadataObservationDocumentType(metaKey1, DocumentType.REGULAR)
        // title
        assertEquals(1, db.queryHistoryMetadata("airpods", 100).size)
        // url
        assertEquals(1, db.queryHistoryMetadata("35377", 100).size)
        // search term
        with(db.queryHistoryMetadata("headset", 100)) {
            assertEquals(1, this.size)
            // view time is zero, since we didn't record it yet.
            assertEquals(0, this[0].totalViewTime)
            // assert that we get the preview image and title
            assertEquals("Are All Wireless Earbuds As Evil As AirPods?", this[0].title)
            assertEquals("https://valkyrie.cdn.ifixit.com/media/2020/02/03121341/bose_soundsport_13.jpg", this[0].previewImageUrl)
        }

        db.noteHistoryMetadataObservationViewTime(metaKey1, 1337)

        // total view time was updated
        with(db.queryHistoryMetadata("headset", 100)) {
            assertEquals(1337, this[0].totalViewTime)
        }

        db.noteHistoryMetadataObservationViewTime(metaKey1, 711)

        with(db.queryHistoryMetadata("headset", 100)) {
            // total view time was updated
            assertEquals(2048, this[0].totalViewTime)
        }

        db.noteHistoryMetadataObservationDocumentType(
            HistoryMetadataKey(
                url = "https://www.youtube.com/watch?v=Cs1b5qvCZ54",
                searchTerm = "путин валдай",
                referrerUrl = "https://yandex.ru/query?путин+валдай",
            ),
            documentType = DocumentType.MEDIA,
            NoteHistoryMetadataObservationOptions(
                ifPageMissing = HistoryMetadataPageMissingBehavior.INSERT_PAGE,
            ),
        )

        // recording view time first, before the document type. either order should be fine.
        val metaKey2 = HistoryMetadataKey(
            url = "https://www.youtube.com/watch?v=fdf4r43g",
            searchTerm = null,
            referrerUrl = null,
        )
        db.noteHistoryMetadataObservationViewTime(
            metaKey2,
            200,
            NoteHistoryMetadataObservationOptions(
                ifPageMissing = HistoryMetadataPageMissingBehavior.INSERT_PAGE,
            ),
        )

        // document type defaults to `regular`.
        with(db.getLatestHistoryMetadataForUrl("https://www.youtube.com/watch?v=fdf4r43g")) {
            assertEquals(200, this!!.totalViewTime)
            assertEquals(DocumentType.REGULAR, this.documentType)
        }

        // able to update document type.
        db.noteHistoryMetadataObservationDocumentType(metaKey2, DocumentType.MEDIA)

        with(db.getLatestHistoryMetadataForUrl("https://www.youtube.com/watch?v=fdf4r43g")) {
            assertEquals(200, this!!.totalViewTime)
            assertEquals(DocumentType.MEDIA, this.documentType)
        }

        // document type isn't reset when updating view time
        db.noteHistoryMetadataObservationViewTime(metaKey2, 300)

        with(db.getLatestHistoryMetadataForUrl("https://www.youtube.com/watch?v=fdf4r43g")) {
            assertEquals(500, this!!.totalViewTime)
            assertEquals(DocumentType.MEDIA, this.documentType)
        }

        assertEquals(2, db.queryHistoryMetadata("youtube", 100).size)
        assertEquals(1, db.queryHistoryMetadata("youtube", 1).size)

        assertEquals(3, db.getHistoryMetadataSince(0L).size)
        assertEquals(3, db.getHistoryMetadataSince(currentTime).size)
        assertEquals(0, db.getHistoryMetadataSince(currentTime + 10000).size)

        assertEquals(0, db.getHistoryMetadataBetween(0, currentTime).size)
        assertEquals(3, db.getHistoryMetadataBetween(currentTime, currentTime + 10000).size)

        db.deleteHistoryMetadataOlderThan(currentTime + 10000)

        assertEquals(0, db.getHistoryMetadataSince(0L).size)

        val metaKeyBad = HistoryMetadataKey(
            url = "invalid-url",
            searchTerm = null,
            referrerUrl = null,
        )
        try {
            db.noteHistoryMetadataObservationViewTime(metaKeyBad, 200)
            assert(false) // should fail
        } catch (e: PlacesApiException) {
            assert(e is PlacesApiException.UrlParseFailed)
        }
    }

    @Test
    fun testRunMaintenanceMetrics() {
        assertNull(PlacesManagerMetrics.runMaintenanceTime.testGetValue())
        assertNull(PlacesManagerMetrics.dbSizeAfterMaintenance.testGetValue())
        db.runMaintenance()
        assertEquals(1, PlacesManagerMetrics.runMaintenanceTime.testGetValue()!!.values.values.sum())
        assertEquals(1, PlacesManagerMetrics.dbSizeAfterMaintenance.testGetValue()!!.values.values.sum())
    }

    @Test
    fun testRegisterWithSyncmanager() {
        val syncManager = SyncManager()

        assertFalse(syncManager.getAvailableEngines().contains("history"))
        assertFalse(syncManager.getAvailableEngines().contains("bookmarks"))

        api.registerWithSyncManager()
        assertTrue(syncManager.getAvailableEngines().contains("history"))
        assertTrue(syncManager.getAvailableEngines().contains("bookmarks"))
    }
}
