/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.fxaclient.internal

import com.sun.jna.Pointer
import com.sun.jna.Structure

import java.util.Arrays

class Profile internal constructor(raw: Raw) {

    val uid: String?
    val email: String?
    val avatar: String?
    val displayName: String?

    internal class Raw(p: Pointer) : Structure(p) {
        @JvmField var uid: Pointer? = null
        @JvmField var email: Pointer? = null
        @JvmField var avatar: Pointer? = null
        @JvmField var displayName: Pointer? = null

        init {
            read()
        }

        override fun getFieldOrder(): List<String> {
            return Arrays.asList("uid", "email", "avatar", "displayName")
        }
    }

    init {
        try {
            this.uid = raw.uid?.getRustString()
            this.email = raw.email?.getRustString()
            this.avatar = raw.avatar?.getRustString()
            this.displayName = raw.displayName?.getRustString()
        } finally {
            FxaClient.INSTANCE.fxa_profile_free(raw.pointer)
        }
    }
}
