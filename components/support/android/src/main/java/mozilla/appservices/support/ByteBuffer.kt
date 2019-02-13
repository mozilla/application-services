/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.support

import com.google.protobuf.CodedInputStream
import com.sun.jna.Pointer
import com.sun.jna.Structure
import java.util.Arrays

/**
 * This is a mapping for the ByteBuffer type from ffi_support.
 *
 * # Caveats:
 *
 * 1. It is for passing data *FROM* Rust code *TO* Kotlin/Java code.
 *    Do *not* use this to pass data in the other direction! Rust code
 *    assumes that it owns ByteBuffers, and will release their memory
 *    when it `Drop`s them.
 *
 *    (Instead, just pass the data and length as two arguments).
 *
 * 2. A ByteBuffer passed into kotlin code must be freed by kotlin
 *    code. The rust code must expose a destructor for this purpose,
 *    and it should be called in the finally block after the data
 *    is read from the CodedInputStream.
 *
 * 3. You almost always should use `ByteBuffer.ByValue` instead
 *    of ByteBuffer. E.g.
 *    `fun mylib_get_stuff(some: X, args: Y): ByteBuffer.ByValue`
 *    for the function returning the ByteBuffer, and
 *    `fun mylib_destroy_bytebuffer(bb: ByteBuffer.ByValue)`.
 */
open class ByteBuffer : Structure() {
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
