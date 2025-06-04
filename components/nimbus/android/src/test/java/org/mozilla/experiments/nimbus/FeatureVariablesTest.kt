/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.experiments.nimbus

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

@RunWith(RobolectricTestRunner::class)
class FeatureVariablesTest {
    private val context: Context
        get() = ApplicationProvider.getApplicationContext()

    @Test
    fun `test values coerce into simple types`() {
        val json = JSONObject(
            """
            {"stringVariable": "string", "intVariable": 3, "booleanVariable": true}
            """.trimIndent(),
        )

        val variables: Variables = JSONVariables(context, json)

        assertEquals(variables.getInt("intVariable"), 3)
        assertEquals(variables.getString("stringVariable"), "string")
        assertEquals(variables.getBool("booleanVariable"), true)
    }

    @Test
    fun `test integer-like text values are not mistakenly used as resource IDs`() {
        val json = JSONObject(
            """
            {"textVariable": "1234"}
            """.trimIndent(),
        )

        val variables: Variables = JSONVariables(context, json)

        // `getIdentifier()` in `ResourcesImpl.java` treats strings that look
        // like integers as valid identifiers, without even checking them,
        // because why not... So we should get a valid string resource ID here,
        // even when it makes no sense.
        assertEquals(variables.getStringResource("textVariable"), 1234)
        // We should expect the value as text, though, and not an error.
        assertEquals(variables.getText("textVariable"), "1234")
    }

    @Test
    fun `test values are null if the wrong type`() {
        val json = JSONObject(
            """
            {"stringVariable": "string", "intVariable": 3, "booleanVariable": true}
            """.trimIndent(),
        )

        val variables: Variables = JSONVariables(context, json)

        assertNull(variables.getString("intVariable"))
        assertNull(variables.getBool("intVariable"))

        assertNull(variables.getInt("stringVariable"))
        assertNull(variables.getBool("stringVariable"))

        assertEquals(variables.getBool("booleanVariable"), true)
        assertNull(variables.getInt("booleanVariable"))
        assertNull(variables.getString("booleanVariable"))
    }

    @Test
    fun `test nested values are make another variables object`() {
        val json = JSONObject(
            """
            {
                "inner": {
                    "stringVariable": "string",
                    "intVariable": 3,
                    "booleanVariable": true
                },
                "really-a-string": "a string"
            }
            """.trimIndent(),
        )

        val outer: Variables = JSONVariables(context, json)

        assertNull(outer.getVariables("not-there"))
        val inner = outer.getVariables("inner")

        assertNotNull(inner)
        assertEquals(inner!!.getInt("intVariable"), 3)
        assertEquals(inner.getString("stringVariable"), "string")
        assertEquals(inner.getBool("booleanVariable"), true)

        assertNull(outer.getVariables("really-a-string"))
    }

    @Test
    fun `test arrays of strings`() {
        val json = JSONObject(
            """
            {
                "empty": [],
                "all-strings": ["x", "y", "z"],
                "some-strings": [1, true, "one", "two", false, 0, [], {}]
            }
            """.trimIndent(),
        )

        val outer: Variables = JSONVariables(context, json)

        assertEquals(outer.getStringList("empty"), listOf<String>())
        assertEquals(outer.getStringList("all-strings"), listOf("x", "y", "z"))
        assertEquals(outer.getStringList("some-strings"), listOf("one", "two"))
    }

    @Test
    fun `test map of strings to raw type (int)`() {
        val json = JSONObject(
            """
            {
                "empty": {},
                "all-strings": {"one": 1, "two": 2},
                "some-strings": {"one": 1, "two": 2, "three": "not at int", "four": true}
            }
            """.trimIndent(),
        )

        val outer = JSONVariables(context, json)

        assertEquals(outer.getIntMap("empty"), mapOf<String, Int>())
        assertEquals(outer.getIntMap("all-strings"), mapOf("one" to 1, "two" to 2))
        assertEquals(outer.getIntMap("some-strings"), mapOf("one" to 1, "two" to 2))
    }

    @Test
    fun `test map of strings to raw type (strings)`() {
        val json = JSONObject(
            """
            {
                "empty": {},
                "all-strings": {"one": "ONE", "two": "TWO"},
                "some-strings": {"one": 1, "two": 2, "three": "THREE", "four": true}
            }
            """.trimIndent(),
        )

        val outer = JSONVariables(context, json)

        assertEquals(outer.getStringMap("empty"), mapOf<String, String>())
        assertEquals(outer.getStringMap("all-strings"), mapOf("one" to "ONE", "two" to "TWO"))
        assertEquals(outer.getStringMap("some-strings"), mapOf("three" to "THREE"))
    }

    @Test
    fun `test map of strings to raw type (bool)`() {
        val json = JSONObject(
            """
            {
                "empty": {},
                "all-bools": {"one": true, "two": false},
                "some-bools": {"one": 1, "two": 2, "three": "THREE", "four": true}
            }
            """.trimIndent(),
        )

        val outer = JSONVariables(context, json)

        assertEquals(outer.getBoolMap("empty"), mapOf<String, String>())
        assertEquals(outer.getBoolMap("all-bools"), mapOf("one" to true, "two" to false))
        assertEquals(outer.getBoolMap("some-bools"), mapOf("four" to true))
    }

    @Test
    fun `test transforming enum keys and values`() {
        val json = JSONObject(
            """
            {
                "num-bools": {"one": true, "two": false},
                "string-num": {"one": "one", "two": "two", "three": "three"},
                "some-nums": ["one", "one", "two", "three"]
            }
            """.trimIndent(),
        )

        val variables: Variables = JSONVariables(context, json)

        assertEquals(
            variables.getBoolMap("num-bools")?.mapKeysAsEnums<NumKey, Boolean>(),
            mapOf(NumKey.one to true, NumKey.two to false),
        )

        assertEquals(
            variables.getStringMap("string-num")?.mapKeysAsEnums<NumKey, String>(),
            mapOf(NumKey.one to "one", NumKey.two to "two"),
        )

        assertEquals(
            variables.getStringMap("string-num")?.mapValuesAsEnums<String, NumKey>(),
            mapOf("one" to NumKey.one, "two" to NumKey.two),
        )

        assertEquals(
            variables.getEnumList<NumKey>("some-nums"),
            listOf(NumKey.one, NumKey.one, NumKey.two),
        )
    }

    @Test
    fun `test ordering of menu items`() {
        val json = JSONObject(
            """
            {
                "items": {
                    "settings": {
                        "label": "Settings",
                        "deepLink": "//settings"
                    },
                    "bookmarks": {
                        "label": "Bookmarks",
                        "deepLink": "//bookmark-list"
                    },
                    "history": {
                        "label": "History",
                        "deepLink": "//history"
                    },
                    "addBookmark": {
                        "label": "Bookmark this page"
                    }
                },
                "item-order": ["settings", "history", "addBookmark", "bookmarks", "open_bad_site"]
            }
            """.trimIndent(),
        )

        val variables: Variables = JSONVariables(context, json)

        // Unpack the variables into a MenuItem data-class.
        // If we can't do the conversion, then we return null.
        @Suppress("ReturnCount")
        fun toMenuItem(inner: Variables): MenuItem? {
            val deepLink = inner.getString("deepLink") ?: return null
            val label = inner.getText("label") ?: return null
            return MenuItem(deepLink, label)
        }
        // Mmm, type safety.
        val items: Map<MenuItemId, MenuItem>? = variables
            .getVariablesMap("items", ::toMenuItem)
            ?.mapKeysAsEnums()

        assertNotNull(items)
        assertEquals(items!!.size, 3)
        // addBookmark wasn't well formed, so we rejected it
        assertFalse(items.containsKey(MenuItemId.addBookmark))

        // Next we test the item ordering, which should all be members of the MenuItemId enum.
        val ordering: List<MenuItemId>? = variables.getEnumList("item-order")
        // open_bad_site doesn't exist, and is filtered out.
        assertEquals(ordering, listOf(MenuItemId.settings, MenuItemId.history, MenuItemId.addBookmark, MenuItemId.bookmarks))
    }
}

@Suppress("EnumNaming", "ktlint:standard:enum-entry-name-case")
enum class MenuItemId {
    settings,
    bookmarks,
    history,
    addBookmark,
}

data class MenuItem(val deepLink: String, val label: String)

@Suppress("EnumNaming", "ktlint:standard:enum-entry-name-case")
enum class NumKey {
    one,
    two,
}
