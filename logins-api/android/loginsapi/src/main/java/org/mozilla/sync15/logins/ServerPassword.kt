/* Copyright 2018 Mozilla
 * Licensed under the Apache License, Version 2.0 (the "License"); you may not use
 * this file except in compliance with the License. You may obtain a copy of the
 * License at http://www.apache.org/licenses/LICENSE-2.0
 * Unless required by applicable law or agreed to in writing, software distributed
 * under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
 * CONDITIONS OF ANY KIND, either express or implied. See the License for the
 * specific language governing permissions and limitations under the License. */
package org.mozilla.sync15.logins

/**
 * Raw password data that is stored by the LoginsStorage implementation.
 */
class ServerPassword (

     /**
      * The unique ID associated with this login.
      *
      * It is recommended that you not make assumptions about its format, but in practice it is
      * typically (but not guaranteed to be) either 12 random Base64URL-safe characters or a
      * UUID-v4 surrounded in curly-braces.
      */
    val id: String,

    val hostname: String,
    val username: String?,

    val password: String,

    /**
     * The HTTP realm, which is the challenge string for HTTP Basic Auth). May be null in the case
     * that this login has a formSubmitURL instead.
     */
    val httpRealm: String? = null,

    /**
     * The formSubmitURL (as a string). This may be null in the case that this login has a
     * httpRealm instead.
     */
    val formSubmitURL: String? = null,

    val timesUsed: Int,

    val timeCreated: Long,
    val timeLastUsed: Long,
    val timePasswordChanged: Long,

    val usernameField: String? = null,
    val passwordField: String? = null
)

