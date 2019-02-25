/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.fxaclient

import com.sun.jna.Pointer
import com.sun.jna.Structure
import mozilla.appservices.fxaclient.rust.FxaClient
import java.util.*

class AccessTokenInfo internal constructor(raw: Raw) {

    val scope: String
    val token: String
    val key: String?
    val expiresAt: Long

    class Raw(p: Pointer) : Structure(p) {
        @JvmField var scope: Pointer? = null
        @JvmField var token: Pointer? = null
        @JvmField var key: Pointer? = null
        @JvmField var expiresAt: Long = 0 // In seconds.

        init {
            read()
        }

        override fun getFieldOrder(): List<String> {
            return Arrays.asList("scope", "token", "key", "expiresAt")
        }
    }

    init {
        try {
            this.scope = raw.scope?.getRustString()!! // This field is always present.
            this.token = raw.token?.getRustString()!! // Ditto.
            this.key = raw.key?.getRustString()
            this.expiresAt = raw.expiresAt
        } finally {
            FxaClient.INSTANCE.fxa_oauth_info_free(raw.pointer)
        }
    }
}
