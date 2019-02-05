/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.fxaclient.internal

import ffi_types.FfiTypes.Profile as RawProfile

class Profile internal constructor(byteBuffer: ByteBuffer.ByValue) {

    val uid: String?
    val email: String?
    val avatar: String?
    val avatarDefault: Boolean
    val displayName: String?

    init {
        try {
            val raw = byteBuffer.asCodedInputStream()?.let {
                RawProfile.parseFrom(it)
            } ?: run {
                // TODO: should throw somehow?
                RawProfile.getDefaultInstance()
            }

            this.uid = if (raw.hasUid()) raw.uid else null
            this.email = if (raw.hasEmail()) raw.email else null
            this.avatar = if (raw.hasAvatar()) raw.avatar else null
            this.avatarDefault = raw.avatarDefault
            this.displayName = if (raw.hasDisplayName()) raw.displayName else null
        } finally {
            FxaClient.INSTANCE.fxa_bytebuffer_free(byteBuffer)
        }
    }
}
