/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.support

import com.google.protobuf.MessageLite
import com.google.protobuf.CodedOutputStream
import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * A helper for converting a protobuf Message into a direct `java.nio.ByteBuffer`
 * and it's length. This avoids a copy when passing data to Rust, when compared
 * to using an `Array<Byte>`
 */

fun <T: MessageLite> T.toNioDirectBuffer(): Pair<ByteBuffer, Int> {
    val len = this.serializedSize
    val nioBuf = ByteBuffer.allocateDirect(len)
    nioBuf.order(ByteOrder.nativeOrder())
    val output = CodedOutputStream.newInstance(nioBuf)
    this.writeTo(output)
    output.checkNoSpaceLeft()
    return Pair(first = nioBuf, second = len)
}
