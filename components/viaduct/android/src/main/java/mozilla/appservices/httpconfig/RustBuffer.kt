/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.httpconfig

import com.google.protobuf.CodedInputStream
import com.google.protobuf.CodedOutputStream
import com.sun.jna.Pointer
import com.sun.jna.Structure
import java.nio.ByteBuffer

/**
 * This is a mapping for the `ffi_support::ByteBuffer` struct.
 *
 * The name differs for two reasons.
 *
 * 1. To that the memory this type manages is allocated from rust code,
 *    and must subsequently be freed by rust code.
 *
 * 2. To avoid confusion with java's nio ByteBuffer, which we use for
 *    passing data *to* Rust without incurring additional copies.
 *
 * # Caveats:
 *
 * 1. It is for receiving data *FROM* Rust, and not the other direction.
 *    RustBuffer doesn't expose a way to inspect its contents from Rust.
 *    See `docs/howtos/passing-protobuf-data-over-ffi.md` for how to do
 *    this instead.
 *
 * 2. A `RustBuffer` passed into kotlin code must be freed by kotlin
 *    code *after* the protobuf message is completely deserialized.
 *
 *    The rust code must expose a destructor for this purpose,
 *    and it should be called in the finally block after the data
 *    is read from the `CodedInputStream` (and not before).
 *
 * 3. You almost always should use `RustBuffer.ByValue` instead
 *    of `RustBuffer`. E.g.
 *    `fun mylib_get_stuff(some: X, args: Y): RustBuffer.ByValue`
 *    for the function returning the RustBuffer, and
 *    `fun mylib_destroy_bytebuffer(bb: RustBuffer.ByValue)`.
 */
@Structure.FieldOrder("len", "data")
open class RustBuffer : Structure() {
    @JvmField var len: Long = 0

    @JvmField var data: Pointer? = null

    @Suppress("TooGenericExceptionThrown")
    fun asCodedInputStream(): CodedInputStream? {
        return this.data?.let {
            // We use a ByteArray instead of a ByteBuffer to avoid triggering the following code path:
            // https://github.com/protocolbuffers/protobuf/blob/e667bf6eaaa2fb1ba2987c6538df81f88500d030/java/core/src/main/java/com/google/protobuf/CodedInputStream.java#L185-L187
            // Bug: https://github.com/protocolbuffers/protobuf/issues/7422
            if (this.len < Int.MIN_VALUE || this.len > Int.MAX_VALUE) {
                throw RuntimeException("len does not fit in a int")
            }
            CodedInputStream.newInstance(it.getByteArray(0, this.len.toInt()))
        }
    }

    fun asCodedOutputStream(): CodedOutputStream? {
        return this.data?.let {
            // We use newSafeInstance through reflection to avoid triggering the following code path:
            // https://github.com/protocolbuffers/protobuf/blob/e667bf6eaaa2fb1ba2987c6538df81f88500d030/java/core/src/main/java/com/google/protobuf/CodedOutputStream.java#L134-L136
            // Bug: https://github.com/protocolbuffers/protobuf/issues/7422
            val method = CodedOutputStream::class.java.getDeclaredMethod("newSafeInstance", ByteBuffer::class.java)
            method.isAccessible = true
            return method.invoke(null, it.getByteBuffer(0, this.len)) as CodedOutputStream
        }
    }

    class ByValue : RustBuffer(), Structure.ByValue
}
