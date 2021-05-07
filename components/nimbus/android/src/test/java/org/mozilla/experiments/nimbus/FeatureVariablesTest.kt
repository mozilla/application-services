/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.experiments.nimbus

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import org.json.JSONObject
import org.junit.Assert.assertEquals
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
        val json = JSONObject("""
            {"stringVariable": "string", "intVariable": 3, "booleanVariable": true}
        """.trimIndent())

        val variables: Variables = JSONVariables(context, json)

        assertEquals(variables.getInt("intVariable"), 3)
        assertEquals(variables.getString("stringVariable"), "string")
        assertEquals(variables.getBool("booleanVariable"), true)
    }

    @Test
    fun `test values are null if the wrong type`() {
        val json = JSONObject("""
            {"stringVariable": "string", "intVariable": 3, "booleanVariable": true}
        """.trimIndent())

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
        val json = JSONObject("""
            {
                "inner": {
                    "stringVariable": "string",
                    "intVariable": 3, 
                    "booleanVariable": true
                },
                "really-a-string": "a string"
            }
        """.trimIndent())

        val outer: Variables = JSONVariables(context, json)

        assertNull(outer.getVariables("not-there"))
        val inner = outer.getVariables("inner")

        assertNotNull(inner)
        assertEquals(inner!!.getInt("intVariable"), 3)
        assertEquals(inner.getString("stringVariable"), "string")
        assertEquals(inner.getBool("booleanVariable"), true)

        assertNull(outer.getVariables("really-a-string"))
    }
}
