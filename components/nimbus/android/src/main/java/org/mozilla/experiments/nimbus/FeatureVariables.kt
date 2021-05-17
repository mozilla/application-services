/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.experiments.nimbus

import android.content.Context
import android.graphics.drawable.Drawable
import org.json.JSONObject

/**
 * `Variables` provides a type safe key-value style interface to configure application features
 *
 * The feature developer requests a typed value with a specific `key`. If the key is present, and
 * the value is of the correct type, then it is returned. If neither of these are true, then `null`
 * is returned.
 *
 * Supported types:
 *
 * Basic:
 *
 *  - `String`
 *  - `Int`
 *  - `Boolean`
 *
 * Resource types. These use `getString` to look up an experiment value, then the app's `Context` to
 * find a resource.
 *
 *  - `StringResource`
 *  - `DrawableResource`
 *
 * ```
 * val config = nimbus.getVariables("submitButton")
 *
 * submitButton.text = config.getText("submitButton.text") ?: R.string.submit_button_label
 * submitButton.color = config.getColor("submitButton.color") ?: R.color.button_default
 *
 * ```
 *
 * Each of the keys needed by feature should be documented in the app's experiment manifest, which
 * will provide enough information the Experimenter to design an experiment.
 */
interface Variables {
    /**
     * Finds a string typed value for this key. If none exists, `null` is returned.
     *
     * N.B. the `key` and type `String` should be listed in the experiment manifest.
     */
    fun getString(key: String): String? = null

    /**
     * Finds a integer typed value for this key. If none exists, `null` is returned.
     *
     * N.B. the `key` and type `Int` should be listed in the experiment manifest.
     */
    fun getInt(key: String): Int? = null

    /**
     * Finds a boolean typed value for this key. If none exists, `null` is returned.
     *
     * N.B. the `key` and type `String` should be listed in the experiment manifest.
     */
    fun getBool(key: String): Boolean? = null

    /**
     * Uses `getString(key: String)` to find the name of a drawable resource. If no value for `key`
     * exists, or no resource named with that value exists, then `null` is returned.
     *
     * N.B. the `key` and type `Image` should be listed in the experiment manifest. The
     * names of the drawable resources should also be listed.
     */
    fun getDrawableResource(key: String): Int? = null

    /**
     * Uses `getString(key: String)` to find the name of a string resource. If no value for `key`
     * exists, or no resource named with that value exists, then `null` is returned.
     *
     * N.B. the `key` and type `LocalizedString` should be listed in the experiment manifest. The
     * names of the string resources should also be listed.
     */
    fun getStringResource(key: String): Int? = null

    /**
     * Uses `getString(key: String)` to find the name of a string resource. If a value exists, and
     * a string resource exists with that name, then returns the string from the resource. If no
     * such resource exists, then return the string value as the text.
     *
     * This is a shorthand for:
     *
     * ```
     * val text: String? = nimbus.getString(key)?.let(context::getString) ?: nimbus.getString(key)
     * ```
     *
     * For strings, this is almost always the right choice.
     *
     * N.B. the `key` and type `LocalizedString` should be listed in the experiment manifest. The
     * names of the string resources should also be listed.
     */
    fun getText(key: String): String? = null

    /**
     * Convenience method for `getDrawableResource(key)?.let { context.getDrawable(it) }`.
     *
     * N.B. the `key` and type `Image` should be listed in the experiment manifest. The
     * names of the drawable resources should also be listed.
     */
    fun getDrawable(key: String): Drawable? = null

    // Get a child configuration object.
    fun getVariables(key: String): Variables? = null
    // This may be important when transforming in to a code generated object.
    fun <T> getVariables(key: String, transform: (Variables) -> T) = getVariables(key)?.let(transform)
}

interface VariablesWithContext : Variables {
    val context: Context
    // Lower level accessors that can come across the FFI.
    // Platform specific types, deserialized from the lower level types.
    override fun getDrawableResource(key: String) = getString(key)?.let(this::asDrawableResource)
    override fun getStringResource(key: String): Int? = getString(key)?.let(this::asStringResource)
    override fun getText(key: String) = getString(key)?.let(this::asText)
    override fun getDrawable(key: String) = getDrawableResource(key)?.let(this::asDrawable)

    // These `as*` methods become useful when transforming values found in JSON to actual values
    // the app will use. They're broken out here so they can be re-used by codegen generating
    // defaults from manifest information.
    fun asText(res: Int) = context.getString(res)
    fun asDrawable(res: Int) = context.getDrawable(res)
    fun asText(string: String) = asStringResource(string)?.let(this::asText) ?: string
    fun asStringResource(string: String) = context.getResource(string, "string")
    fun asDrawableResource(string: String) = context.getResource(string, "drawable")
}

// Get a resource Int if it exists from the context resources.
// Here we're using it for icons and strings.
// This will help us look after translations, dark mode, screen size, pixel density etc etc.
private fun Context.getResource(resName: String, defType: String): Int? {
    val res = resources.getIdentifier(resName, defType, packageName)
    return if (res != 0) {
        res
    } else {
        null
    }
}

/**
 * A thin wrapper around the JSON produced by the `get_feature_config_variables_json(feature_id)` call, useful
 * for configuring a feature, but without needing the developer to know about experiment specifics.
 */
class JSONVariables(
    override val context: Context,
    private val json: JSONObject = JSONObject()
) : VariablesWithContext {
    // These `get*` methods get values from the wrapped JSON object, and transform them using the
    // `as*` methods.
    override fun getString(key: String) = json.value<String>(key)

    override fun getInt(key: String) = json.value<Int>(key)

    override fun getBool(key: String) = json.value<Boolean>(key)

    // Methods used to get sub-objects. We immediately re-wrap an JSON object if it exists.
    override fun getVariables(key: String) = json.value<JSONObject>(key)?.let { JSONVariables(context, it) }
}

// A typed getter. If the key is not present, or the value is JSONNull, or the wrong type
// returns `null`.
private inline fun <reified T> JSONObject.value(key: String): T? {
    if (!this.isNull(key)) {
        return this.get(key) as? T
    }
    return null
}

// Another implementation of `Variables` may just return null for everything.
class NullVariables : Variables {
    companion object {
        val instance = NullVariables()
    }
}
