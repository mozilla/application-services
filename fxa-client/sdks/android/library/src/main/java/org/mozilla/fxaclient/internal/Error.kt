/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.fxaclient.internal

import com.sun.jna.Pointer
import com.sun.jna.Structure
import java.util.Arrays

internal open class Error : Structure() {

    class ByReference : Error(), Structure.ByReference

    @JvmField var code: Int = 0
    @JvmField var message: Pointer? = null

    init {
        read()
    }

    /**
     * Does this represent failure?
     */
    fun isFailure(): Boolean {
        return this.code != 0
    }

    /**
     * Get and consume the error message, or null if there is none,
     */
    fun consumeMessage(): String? {
        val p = this.message
        this.message = null
        return p?.getAndConsumeString()
    }

    /** Be sure to call this when the error is definitely no longer in use. */
    fun ensureConsumed() {
        this.consumeMessage()
    }

    override fun getFieldOrder(): List<String> {
        return Arrays.asList("code", "message")
    }
}
