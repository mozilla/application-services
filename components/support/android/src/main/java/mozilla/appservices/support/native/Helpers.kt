/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.support.native

import com.google.protobuf.CodedOutputStream
import com.google.protobuf.MessageLite
import com.sun.jna.Library
import com.sun.jna.Native
import java.lang.reflect.Proxy
import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * A helper for converting a protobuf Message into a direct `java.nio.ByteBuffer`
 * and it's length. This avoids a copy when passing data to Rust, when compared
 * to using an `Array<Byte>`
 */

fun <T : MessageLite> T.toNioDirectBuffer(): Pair<ByteBuffer, Int> {
    val len = this.serializedSize
    val nioBuf = ByteBuffer.allocateDirect(len)
    nioBuf.order(ByteOrder.nativeOrder())
    val output = CodedOutputStream.newInstance(nioBuf)
    this.writeTo(output)
    output.checkNoSpaceLeft()
    return Pair(first = nioBuf, second = len)
}

sealed class MegazordError : Exception {
    val libName: String
    constructor(libName: String, msg: String) : super(msg) {
        this.libName = libName
    }
    constructor(libName: String, msg: String, cause: Throwable) : super(msg, cause) {
        this.libName = libName
    }
}

class MegazordNotAvailable(
    libName: String,
    cause: UnsatisfiedLinkError
) : MegazordError(libName, "Failed to locate megazord library: '$libName'", cause)

class IncompatibleMegazordVersion(
    libName: String,
    val libVersion: String,
    val mzVersion: String
) : MegazordError(
    libName,
    "Incompatible megazord version: library \"$libName\" was compiled expecting " +
    "app-services version \"$libVersion\", but the megazord provides version \"$mzVersion\""
)

class MegazordNotInitialized(libName: String) : MegazordError(
    libName,
    "The application-services megazord has not yet been initialized, but is needed by \"$libName\""
)

fun assertMegazordLibVersionsCompatible(libName: String, libVersion: String, mzVersion: String) {
    // We require exact equality, since we don't perform a major version
    // bump if we change the ABI. In practice, this seems unlikely to
    // cause problems, but we could come up with a scheme if this proves annoying.
    if (libVersion != mzVersion) {
        throw IncompatibleMegazordVersion(libName, libVersion, mzVersion)
    }
}

/**
 * Determine the megazord library name, and check that it's version is
 * compatible with the version of our bindings. Returns the megazord
 * library name.
 *
 * Note: This is only public because it's called by an inline function.
 * It should not be called by consumers.
 */
fun megazordCheck(libName: String, libVersion: String): String {
    val mzLibrary = System.getProperty("mozilla.appservices.megazord.library")
        ?: throw MegazordNotInitialized(libName)

    // Assume it's properly initialized if it's been initialized at all
    val mzVersion = System.getProperty("mozilla.appservices.megazord.version")!!

    // We require exact equality, since we don't perform a major version
    // bump if we change the ABI. In practice, this seems unlikely to
    // cause problems, but we could come up with a scheme if this proves annoying.
    if (libVersion != mzVersion) {
        throw IncompatibleMegazordVersion(libName, libVersion, mzVersion)
    }
    return mzLibrary
}

/**
 * Contains all the boilerplate for loading a
 *
 * Indirect as in, we aren't using JNA direct mapping. Eventually we'd
 * like to (it's faster), but that's a problem for another day.
 */
inline fun <reified Lib : Library> loadIndirect(libName: String, libVersion: String): Lib {
    val mzLibrary = megazordCheck(libName, libVersion)
    return try {
        Native.load<Lib>(mzLibrary, Lib::class.java)
    } catch (e: UnsatisfiedLinkError) {
        // TODO: This should probably be a hard error now, right?
        Proxy.newProxyInstance(
            Lib::class.java.classLoader,
            arrayOf(Lib::class.java)) { _, _, _ ->
            throw MegazordNotAvailable(libName, e)
        } as Lib
    }
}
