/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.experiments.nimbus

import android.content.Context
import android.content.res.Resources.NotFoundException
import android.graphics.drawable.Drawable
import org.json.JSONArray
import org.json.JSONObject
import org.mozilla.experiments.nimbus.internal.NimbusFeatureException
import org.mozilla.experiments.nimbus.internal.mapValuesNotNull

/**
 * `Variables` provides a type safe key-value style interface to configure application features
 *
 * The feature developer requests a typed value with a specific `key`. If the key is present, and
 * the value is of the correct type, then it is returned. If neither of these are true, then `null`
 * is returned.
 *
 * ## Supported types:
 *
 * ### Primitive types:
 *
 *  - `String`
 *  - `Int`
 *  - `Boolean`
 *
 * ### Types coerced from `String` values:
 *
 *  - `Enum<T>`
 *  - Resources
 *      These use `getString` to look up an experiment value, then the app's `Context` to
 *      find a resource. These are `Text` and `Drawable`.
 *
 * ```
 * val config = nimbus.getVariables("submitButton")
 *
 * submitButton.text = config.getText("submitButton.text") ?: R.string.submit_button_label
 * submitButton.drawable = config.getDrawable("submitButton.color") ?: context.getDrawable(R.drawable.button_default)
 * ```
 *
 * ### Nested `Variables`
 *
 * As `JSONObject`s can contain other `JSONObject`s, then so `Variables` can contain other `Variables`.
 *
 * Convenience methods are provided to map these inner Variables into richer types.
 *
 * ### Structural types
 *
 * For all types, corresponding `List` and `Map` methods are available.
 *
 * In the case of all Maps, they are returned as `Map<String, T>`.
 *
 * ### Enums
 *
 * String coercion to Enums are supported, but provided as extension methods to
 *
 *  - `Variables.getEnumList<E>`,
 *  - `Map<K, String>.mapKeysAsEnum`
 *  - `Map<String, V>.mapValuesAsEnum`
 *
 * Special
 *
 * Each of the keys needed by feature should be documented in the app's experiment manifest, which
 * will provide enough information the Experimenter to design an experiment.
 */
interface Variables {
    val context: Context

    /**
     * Finds a string typed value for this key. If none exists, `null` is returned.
     *
     * N.B. the `key` and type `String` should be listed in the experiment manifest.
     */
    fun getString(key: String): String? = null

    /**
     * Find an array for this key, and returns all the strings in that array. If none exists, `null`
     * is returned.
     */
    fun getStringList(key: String): List<String>? = null

    /**
     * Find a map for this key, and returns a map containing all the entries that have strings
     * as their values. If none exists, then `null` is returned.
     */
    fun getStringMap(key: String): Map<String, String>? = null

    fun asStringMap(): Map<String, String>? = null

    /**
     * Finds a integer typed value for this key. If none exists, `null` is returned.
     *
     * N.B. the `key` and type `Int` should be listed in the experiment manifest.
     */
    fun getInt(key: String): Int? = null

    /**
     * Find an array for this key, and returns all the integers in that array. If none exists, `null`
     * is returned.
     */
    fun getIntList(key: String): List<Int>? = null

    /**
     * Find a map for this key, and returns a map containing all the entries that have integers
     * as their values. If none exists, then `null` is returned.
     */
    fun getIntMap(key: String): Map<String, Int>? = null

    fun asIntMap(): Map<String, Int>? = null

    /**
     * Finds a boolean typed value for this key. If none exists, `null` is returned.
     *
     * N.B. the `key` and type `String` should be listed in the experiment manifest.
     */
    fun getBool(key: String): Boolean? = null

    /**
     * Find an array for this key, and returns all the booleans in that array. If none exists, `null`
     * is returned.
     */
    fun getBoolList(key: String): List<Boolean>? = null

    /**
     * Find a map for this key, and returns a map containing all the entries that have booleans
     * as their values. If none exists, then `null` is returned.
     */
    fun getBoolMap(key: String): Map<String, Boolean>? = null

    fun asBoolMap(): Map<String, Boolean>? = null

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
     * Uses `getStringList(key: String)` to get a list of strings, then coerces the
     * strings in the list into localized text strings.
     */
    fun getTextList(key: String): List<String>? = null

    /**
     * Uses `getStringMap(key: String)` to get a map of strings, then coerces the
     * string values into localized text strings.
     */
    fun getTextMap(key: String): Map<String, String>? = null

    /**
     * Convenience method for `getDrawableResource(key)?.let { context.getDrawable(it) }`.
     *
     * N.B. the `key` and type `Image` should be listed in the experiment manifest. The
     * names of the drawable resources should also be listed.
     */
    fun getDrawable(key: String): Res<Drawable>? = null

    /**
     * Uses `getStringList(key: String)` to get a list of strings, then coerces the
     * strings in the list into Drawables. Values that cannot be coerced are omitted.
     */
    fun getDrawableList(key: String): List<Res<Drawable>>? = null

    /**
     * Uses `getStringList(key: String)` to get a list of strings, then coerces the
     * values into Drawables. Values that cannot be coerced are omitted.
     */
    fun getDrawableMap(key: String): Map<String, Res<Drawable>>? = null

    // Get a child configuration object.

    /**
     * Gets a nested `JSONObject` value for this key, and creates a new `Variables` object. If
     * the value at the key is not a JSONObject, then return `null`.
     */
    fun getVariables(key: String): Variables? = null

    /**
     * Gets a list value for this key, and transforms all `JSONObject`s in the list into `Variables`.
     *
     * If the value isn't a list, then returns `null`. Items in the list that are not `JSONObject`s
     * are omitted from the final list.
     */
    fun getVariablesList(key: String): List<Variables>? = null

    /**
     * Gets a map value for this key, and transforms all `JSONObject`s that are values into `Variables`.
     *
     * If the value isn't a `JSONObject`, then returns `null`. Values in the map that are not `JSONObject`s
     * are omitted from the final map.
     */
    fun getVariablesMap(key: String): Map<String, Variables>? = null

    fun asVariablesMap(): Map<String, JSONVariables>? = null

    // This may be important when transforming into a code generated object.
    /**
     * Get a `Variables` object for this key, and transforms it to a `T`. If this is not possible, then the `transform` should
     * return `null`.
     */
    fun <T> getVariables(key: String, transform: (Variables) -> T?) = getVariables(key)?.let(transform)

    /**
     * Uses `getVariablesList(key)` then transforms each `Variables` into a `T`.
     * If any item cannot be transformed, it is skipped.
     */
    fun <T> getVariablesList(key: String, transform: (Variables) -> T?): List<T>? =
        getVariablesList(key)?.mapNotNull(transform)

    /**
     * Uses `getVariablesMap(key)` then transforms each `Variables` value into a `T`.
     * If any value cannot be transformed, it is skipped.
     */
    fun <T> getVariablesMap(key: String, transform: (Variables) -> T?): Map<String, T>? =
        getVariablesMap(key)?.mapValuesNotNull(transform)

    /**
     * Finds a string typed value for this key. If none exists, `null` is returned.
     *
     * N.B. the `key` and type `String` should be listed in the experiment manifest.
     */
    fun <T> getString(key: String, transform: (String) -> T?): T? =
        getString(key)?.let(transform)

    /**
     * Find an array for this key, and returns all the strings in that array. If none exists, `null`
     * is returned.
     */
    fun <T> getStringList(key: String, transform: (String) -> T?): List<T>? =
        getStringList(key)?.mapNotNull(transform)

    /**
     * Find a map for this key, and returns a map containing all the entries that have strings
     * as their values. If none exists, then `null` is returned.
     */
    fun <T> getStringMap(key: String, transform: (String) -> T?): Map<String, T>? =
        getStringMap(key)?.mapValuesNotNull(transform)

    /**
     * Synonym for [getDrawable(key: String)], for easier code generation.
     */
    fun getImage(key: String): Res<Drawable>? = getDrawable(key)

    /**
     * Synonym for [getDrawableList(key: String)], for easier code generation.
     */
    fun getImageList(key: String): List<Res<Drawable>>? = getDrawableList(key)

    /**
     * Synonym for [getDrawableMap(key: String)], for easier code generation.
     */
    fun getImageMap(key: String): Map<String, Res<Drawable>>? = getDrawableMap(key)
}

inline fun <reified T : Enum<T>> String.asEnum(): T? = try {
    enumValueOf<T>(this)
} catch (e: IllegalArgumentException) {
    null
}

/**
 * Uses `getString(key: String)` to find a string value for the given key, and coerce it into
 * the `Enum<T>`. If the value doesn't correspond to a variant of the type T, then `null` is
 * returned.
 */
inline fun <reified T : Enum<T>> Variables.getEnum(key: String): T? =
    getString(key)?.asEnum<T>()

/**
 * Uses `getStringList(key: String)` to find a value that is a list of strings for the given key,
 * and coerce each item into an `Enum<T>`.
 * If the value doesn't correspond to a variant of the list, then `null` is
 * returned.
 * Items of the list that are not underlying strings, or cannot be coerced into variants,
 * are omitted.
 */
inline fun <reified T : Enum<T>> Variables.getEnumList(key: String): List<T>? =
    getStringList(key)?.mapNotNull { it.asEnum<T>() }

/**
 * Uses `getStringMap(key: String)` to find a value that is a map of strings for the given key, and
 * coerces each value into an `Enum<T>`.
 *
 * If the value doesn't correspond to a variant of the list, then `null` is
 * returned.
 * Values that are not underlying strings, or cannot be coerced into variants,
 *
 * are omitted.
 */
inline fun <reified T : Enum<T>> Variables.getEnumMap(key: String): Map<String, T>? =
    getStringMap(key)?.mapValuesAsEnums<String, T>()

/**
 * Convenience extension method for maps with `String` keys.
 *
 * If a `String` key cannot be coerced into a variant of the given Enum, then the entry is
 * omitted.
 *
 * This is useful in combination with `getVariablesMap(key, transform)`:
 *
 * ```
 * val variables = nimbus.getVariables("menu-feature")
 * val menuItems: Map<MenuItemId, MenuItem> = variables
 *      .getVariablesMap("items", ::toMenuItem)
 *      ?.mapKeysAsEnums()
 *
 * val menuItemOrder = variables.getEnumList<MenuItemId>("item-order")
 * ```
 */
inline fun <reified K : Enum<K>, V> Map<String, V>.mapKeysAsEnums(): Map<K, V> =
    this.entries.mapNotNull { e ->
        e.key.asEnum<K>()?.let { key ->
            key to e.value
        }
    }.toMap()

/**
 * Convenience extension method for maps with `String` values.
 *
 * If a `String` value cannot be coerced into a variant of the given Enum, then the entry is
 * omitted.
 */
inline fun <K, reified V : Enum<V>> Map<K, String>.mapValuesAsEnums(): Map<K, V> =
    this.entries.mapNotNull { e ->
        e.value.asEnum<V>()?.let { value ->
            e.key to value
        }
    }.toMap()

interface VariablesWithContext : Variables {
    // Lower level accessors that can come across the FFI.
    // Platform specific types, deserialized from the lower level types.
    override fun getDrawableResource(key: String) = getString(key)?.let(this::asDrawableResource)
    override fun getStringResource(key: String): Int? = getString(key)?.let(this::asStringResource)
    override fun getText(key: String) = getString(key)?.let(this::asText)
    override fun getTextList(key: String) = getStringList(key)?.mapNotNull(this::asText)
    override fun getTextMap(key: String): Map<String, String>? = getStringMap(key)?.mapValuesNotNull(this::asText)
    override fun getDrawable(key: String) = getDrawableResource(key)?.let(this::asDrawable)
    override fun getDrawableList(key: String) = getStringList(key)?.mapNotNull(this::asDrawableResource)?.mapNotNull(this::asDrawable)
    override fun getDrawableMap(key: String) = getStringMap(key)?.mapValuesNotNull(this::asDrawableResource)?.mapValuesNotNull(this::asDrawable)

    // These `as*` methods become useful when transforming values found in JSON to actual values
    // the app will use. They're broken out here so they can be re-used by codegen generating
    // defaults from manifest information.
    fun asText(res: Int) = context.getString(res)
    fun asDrawable(res: Int): Res<Drawable> = DrawableRes(context, res)
    fun asText(string: String): String =
        try {
            // It's possible `asStringResource` will return an ID for a
            // resource that doesn't actually exist, and `asText(Int)` might
            // throw as a result.
            asStringResource(string)?.let(this::asText) ?: string
        } catch (e: NotFoundException) {
            string
        }
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
    private val json: JSONObject = JSONObject(),
) : VariablesWithContext {
    // These `get*` methods get values from the wrapped JSON object, and transform them using the
    // `as*` methods.
    override fun getString(key: String) = json.value<String>(key)
    override fun getStringList(key: String) = json.values<String>(key)
    override fun getStringMap(key: String) = json.mapOf<String>(key)
    override fun asStringMap() = json.asMap<String>()

    override fun getInt(key: String) = json.value<Int>(key)
    override fun getIntList(key: String) = json.values<Int>(key)
    override fun getIntMap(key: String) = json.mapOf<Int>(key)
    override fun asIntMap() = json.asMap<Int>()

    override fun getBool(key: String) = json.value<Boolean>(key)
    override fun getBoolList(key: String) = json.values<Boolean>(key)
    override fun getBoolMap(key: String) = json.mapOf<Boolean>(key)
    override fun asBoolMap() = json.asMap<Boolean>()

    // Methods used to get sub-objects. We immediately re-wrap an JSON object if it exists.
    override fun getVariables(key: String) = json.value<JSONObject>(key)?.let(this::asVariables)

    override fun getVariablesList(key: String) =
        json.values<JSONObject>(key)?.let { jsonObjects ->
            jsonObjects.map(this::asVariables)
        }

    override fun getVariablesMap(key: String): Map<String, Variables>? =
        json.mapOf<JSONObject>(key)?.mapValuesNotNull(this::asVariables)

    override fun asVariablesMap() = json.asMap<JSONObject>()?.mapValuesNotNull(this::asVariables)

    private fun asVariables(json: JSONObject) = JSONVariables(context, json)
}

// A typed getter. If the key is not present, or the value is JSONNull, or the wrong type
// returns `null`.
private inline fun <reified T> JSONObject.value(key: String): T? {
    if (!this.isNull(key)) {
        return this.get(key) as? T
    }
    return null
}

private inline fun <reified T> JSONObject.values(key: String): List<T>? =
    this.value<JSONArray>(key)?.values<T>()

private inline fun <reified T> JSONArray.values(): List<T> {
    val list = mutableListOf<T>()
    for (i in 0 until this.length()) {
        (this[i] as? T)?.let(list::add)
    }
    return list
}

private inline fun <reified T> JSONObject.mapOf(key: String) =
    this.value<JSONObject>(key)?.asMap<T>()

private inline fun <reified T> JSONObject.asMap(): Map<String, T>? {
    val map = mutableMapOf<String, T>()
    this.keys().forEach { key ->
        this.value<T>(key)?.let { value -> map[key] = value }
    }
    return map
}

// Another implementation of `Variables` may just return null for everything.
class NullVariables : Variables {
    override val context: Context
        get() = this._context
            ?: throw NimbusFeatureException(
                """
                Nimbus hasn't been initialized yet.

                Calling NullVariables.instance.setContext(context) earlier in the app startup will
                cause this error to go away, but won't fix the problem.

                The best remedy for this error is to initialize Nimbus earlier in the start up sequence.
                """.trimIndent(),
            )

    private var _context: Context? = null

    fun setContext(context: Context) {
        this._context = context.applicationContext
    }

    companion object {
        val instance: NullVariables by lazy { NullVariables() }
    }
}

/**
 * Accessor object to allow callers access to the resource identifier as well as the
 * convenience of getting the underlying resource.
 *
 * It is intended as a uniform way of accessing different resource types
 * bundled with the app, through Nimbus.
 */
interface Res<T> {
    /**
     * The resource identifier
     */
    val resourceId: Int

    /**
     * The actual resource.
     */
    val resource: T

    /**
     * The resource name used to identify this resource.
     */
    val resourceName: String

    companion object {
        fun drawable(context: Context, resId: Int): Res<Drawable> =
            DrawableRes(context, resId)
        fun string(resId: Int) =
            StringHolder(resId, null)
        fun string(literal: String) =
            StringHolder(null, literal)
    }
}

internal class DrawableRes(
    private val context: Context,
    override val resourceId: Int,
) : Res<Drawable> {
    override val resource: Drawable
        get() = context.resources.getDrawable(resourceId, context.theme)

    override val resourceName: String
        @Suppress("TooGenericExceptionCaught")
        get() = try { context.resources.getResourceName(resourceId) } catch (e: Throwable) { "unknown" }
}

class StringHolder(
    private val resourceId: Int?,
    private val literal: String?,
) {

    @Suppress("ExceptionRaisedInUnexpectedLocation")
    fun toString(context: Context): String =
        resourceId
            ?.let { context.getString(it) }
            ?: literal
            ?: throw NimbusFeatureException("Internal Nimbus exception: A Text string from the FML is missing.")
}
