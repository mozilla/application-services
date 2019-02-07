/* Any copyright is dedicated to the Public Domain.
   http://creativecommons.org/publicdomain/zero/1.0/ */

package org.mozilla.places


import org.junit.After
import org.junit.rules.TemporaryFolder
import org.junit.Rule
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.junit.Test
import org.junit.Assert.*
import org.junit.Before


@RunWith(RobolectricTestRunner::class)
@Config(manifest = Config.NONE)
class PlacesConnectionTest {
    @Rule
    @JvmField
    val dbFolder = TemporaryFolder()

    lateinit var db: PlacesConnection

    @Before
    fun initDB() {
        db = PlacesConnection(path = dbFolder.newFile().absolutePath)
    }

    @After
    fun closeDB() {
        db.close()
    }

    // Basically equivalent to test_get_visited in rust, but exercises the FFI,
    // as well as the handling of invalid urls.
    @Test
    fun testGetVisited() {

        val unicodeInPath = "http://www.example.com/tëst😀abc"
        val escapedUnicodeInPath = "http://www.example.com/t%C3%ABst%F0%9F%98%80abc";

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
                "$escapedUnicodeInDomain/2"
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
                Pair("$escapedUnicodeInDomain/1", true)
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
        } catch (e: PlacesException) {
            assert(e is UrlParseFailed)
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
                "https://news.ycombinator.com/"
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

}

