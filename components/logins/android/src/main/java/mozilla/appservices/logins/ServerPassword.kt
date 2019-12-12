/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.logins
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject

/**
 * Raw password data that is stored by the LoginsStorage implementation.
 */
data class ServerPassword(

    /**
     * The unique ID associated with this login.
     *
     * It is recommended that you not make assumptions about its format, but in practice it is
     * typically (but not guaranteed to be) either 12 random Base64URL-safe characters or a
     * UUID-v4 surrounded in curly-braces.
     */
   val id: String,

    /**
     * The hostname this record corresponds to. It is an error to
     * attempt to insert or update a record to have a blank hostname.
     */
    val hostname: String,

    val username: String,

    /**
     * The password field of this record. It is an error to attempt to insert or update
     * a record to have a blank password.
     */
    val password: String,

    /**
     * The HTTP realm, which is the challenge string for HTTP Basic Auth. May be null in the case
     * that this login has a formSubmitURL instead.
     */
    val httpRealm: String? = null,

    /**
     * The formSubmitURL (as a string). This may be null in the case that this login has a
     * httpRealm instead.
     */
    val formSubmitURL: String? = null,

    /**
     * Number of times this password has been used.
     */
    val timesUsed: Int = 0,

    /**
     * Time of creation in milliseconds from the unix epoch.
     */
    val timeCreated: Long = 0L,

    /**
     * Time of last use in milliseconds from the unix epoch.
     */
    val timeLastUsed: Long = 0L,

    /**
     * Time of last password change in milliseconds from the unix epoch.
     */
    val timePasswordChanged: Long = 0L,

    val usernameField: String,
    val passwordField: String
) {

    fun toJSON(): JSONObject {
        val o = JSONObject()
        o.put("id", this.id)
        o.put("hostname", this.hostname)
        o.put("password", password)
        o.put("timesUsed", timesUsed)
        o.put("timeCreated", timeCreated)
        o.put("timeLastUsed", timeLastUsed)
        o.put("timePasswordChanged", timePasswordChanged)
        o.put("username", username)
        if (httpRealm != null) {
            o.put("httpRealm", httpRealm)
        }
        if (formSubmitURL != null) {
            o.put("formSubmitURL", formSubmitURL)
        }
        o.put("usernameField", usernameField)
        o.put("passwordField", passwordField)
        return o
    }

    companion object {
        fun fromJSON(jsonObject: JSONObject): ServerPassword {

            fun stringOrNull(key: String): String? {
                return try {
                    jsonObject.getString(key)
                } catch (e: JSONException) {
                    null
                }
            }

            return ServerPassword(
                    id = jsonObject.getString("id"),

                    hostname = jsonObject.getString("hostname"),
                    password = jsonObject.getString("password"),
                    username = jsonObject.getString("username"),

                    httpRealm = stringOrNull("httpRealm"),
                    formSubmitURL = stringOrNull("formSubmitURL"),

                    usernameField = jsonObject.getString("usernameField"),
                    passwordField = jsonObject.getString("passwordField"),

                    timesUsed = jsonObject.getInt("timesUsed"),

                    timeCreated = jsonObject.getLong("timeCreated"),
                    timeLastUsed = jsonObject.getLong("timeLastUsed"),
                    timePasswordChanged = jsonObject.getLong("timePasswordChanged")
            )
        }

        fun fromJSON(jsonText: String): ServerPassword {
            return fromJSON(JSONObject(jsonText))
        }

        fun fromJSONArray(jsonArrayText: String): List<ServerPassword> {
            val result: MutableList<ServerPassword> = mutableListOf()
            val array = JSONArray(jsonArrayText)
            for (index in 0 until array.length()) {
                result.add(fromJSON(array.getJSONObject(index)))
            }
            return result
        }
    }
}
