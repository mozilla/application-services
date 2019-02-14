/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.fxaclient.internal

import com.google.protobuf.CodedInputStream
import com.sun.jna.Pointer
import com.sun.jna.Structure
import java.util.*

internal open class ByteBuffer : Structure() {
    @JvmField var len: Long = 0
    @JvmField var data: Pointer? = null

    init {
        read()
    }

    override fun getFieldOrder(): List<String> {
        return Arrays.asList("len", "data")
    }

    fun asCodedInputStream(): CodedInputStream? {
        return this.data?.let {
            CodedInputStream.newInstance(it.getByteBuffer(0, this.len))
        }
    }

    class ByValue : ByteBuffer(), Structure.ByValue
}
