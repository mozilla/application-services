/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.fxaclient.internal

import com.sun.jna.Pointer
import com.sun.jna.Structure

import java.util.Arrays

class OAuthInfo internal constructor(raw: Raw) {

    val accessToken: String?
    val keys: String?
    val scope: String?

    class Raw(p: Pointer) : Structure(p) {
        @JvmField var accessToken: Pointer? = null
        @JvmField var keys: Pointer? = null
        @JvmField var scope: Pointer? = null

        init {
            read()
        }

        override fun getFieldOrder(): List<String> {
            return Arrays.asList("accessToken", "keys", "scope")
        }
    }

    init {
        try {
            this.accessToken = raw.accessToken?.getRustString()
            this.keys = raw.keys?.getRustString()
            this.scope = raw.scope?.getRustString()
        } finally {
            FxaClient.INSTANCE.fxa_oauth_info_free(raw.pointer)
        }
    }
}
