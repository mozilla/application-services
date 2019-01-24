/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.fxaclient.internal

import com.sun.jna.Pointer
import com.sun.jna.PointerType
import java.lang.RuntimeException
import java.util.concurrent.atomic.AtomicLong
import java.util.concurrent.atomic.AtomicReference

/**
 * Base class that wraps a Long representing a handle to a Rust object.
 * This class implements [AutoCloseable] but does not provide an implementation, forcing all
 * subclasses to implement it.
 */
abstract class RustObject internal constructor(handle: Long = 0L) : AutoCloseable {
    private var rawHandle: AtomicLong = AtomicLong(handle)

    val isConsumed: Boolean
        get() = this.rawHandle.get() == 0L

    /**
     * Gets the handle, or throws [RustObjectConsumed] if it's already consumed.
     *
     * Note that use of this should be synchronized, as it is in [RustObject.rustCall] or
     * [RustObject.nullableRustCall], to avoid use after free if `validPointer` is called on one
     * thread before something releases it on another.
     */
    internal fun validHandle(): Long {
        return this.rawHandle.get().let {
            if (it != 0L) {
                it
            } else {
                throw RustObjectConsumed()
            }
        }
    }

    /**
     * Consumes the handle, or throws [RustObjectConsumed] if it's already consumed.
     */
    internal fun consumeHandle(): Long {
        return this.consumeHandleOrNull() ?: throw RustObjectConsumed()
    }

    /**
     * Consumes the handle, or returns null if it's already consumed.
     */
    private fun consumeHandleOrNull(): Long? {
        return this.rawHandle.getAndSet(0L).let {
            if (it != 0L) {
                it
            } else {
                null
            }
        }
    }

    /**
     * Override this as a call to the destructor for `T`.
     *
     * Note: Synchronization is not required, as we perform it internally.
     */
    protected abstract fun destroy(p: Long)

    /**
     * Release native resources owned by this RustObject.
     *
     * Unlike many RustObject functions, this is allowed after the RustObject has been consumed,
     * and will not throw [RustObjectConsumed].
     */
    @Synchronized
    override fun close() {
        // Note: `close` is allowed even if we've consumed the pointer already.
        // Also note, AtomicReference doesn't free us from needing `Synchronized`, since
        // rust may be using it on another thread.
        this.consumeHandleOrNull()?.let { destroy(it) }
    }

    /**
     * Helper to call into rust and translate the rust error into exceptions.
     *
     * Assumes that the rust function it calls returns null only on errors, so does a non-null
     * assertion on the result. Use [RustObject.nullableRustCall] for cases where this is not
     * desired.
     *
     * Additionally, synchronizes on `this`. Use [unlockedRustCall] if you're completely certain
     * that you neither need or want synchronization.
     */
    internal inline fun <U> rustCall(callback: (Error.ByReference) -> U?): U {
        // Not sure if I can use @Synchronized in an `inline fun`
        return synchronized(this) {
            unlockedRustCall(callback)
        }
    }

    /** Helper to call into rust and translate the rust error into exceptions.
     *
     * Assumes the rust function may return null even on success. See [RustObject.rustCall]
     * if null is only returned on errors.
     *
     * Additionally, synchronizes on `this`. Use [unlockedNullableRustCall] if you're completely
     * certain that you neither need or want synchronization.
     */
    internal inline fun <U> nullableRustCall(callback: (Error.ByReference) -> U?): U? {
        return synchronized(this) {
            unlockedNullableRustCall(callback)
        }
    }
}

/**
 * Error thrown when a RustObject is used after it has logically been consumed.
 */
open class RustObjectConsumed: RuntimeException("The RustObject has already been consumed!")

/**
 * Helper to call into rust and translate the rust error into exceptions.  Use when you are certain
 * you do not need synchronization.
 *
 * Assumes the rust function may return null even on success. See [unlockedRustCall]
 * if null is only returned on errors.
 */
internal inline fun <U> unlockedNullableRustCall(callback: (Error.ByReference) -> U?): U? {
    val e = Error.ByReference()
    try {
        val ret = callback(e)
        if (e.isFailure()) {
            throw FxaException.fromConsuming(e)
        }
        return ret
    } finally {
        // This only matters if `callback` throws or does a non-local return, which
        // we currently don't do.
        e.ensureConsumed()
    }
}

/**
 * Helper to call into rust and translate the rust error into exceptions. Use when you are certain
 * you do not need synchronization.
 *
 * Assumes that the rust function it calls returns null only on errors, so does a non-null
 * assertion on the result. Use [unlockedNullableRustCall] for cases where this is not
 * desired.
 */
internal inline fun <U> unlockedRustCall(callback: (Error.ByReference) -> U?): U {
    return unlockedNullableRustCall(callback)!!
}

private const val RUST_STRING_ENCODING = "utf8"

/**
 * Helper to read a null terminated String out of the Pointer and free it.
 *
 * Important: Do not use this pointer after this! For anything!
 */
internal fun Pointer.getAndConsumeString(): String {
    try {
        return this.getRustString()
    } finally {
        FxaClient.INSTANCE.fxa_str_free(this)
    }
}

/**
 * Helper to read a null terminated string out of the pointer.
 *
 * Important: doesn't free the pointer, use [getAndConsumeString] for that!
 */
internal fun Pointer.getRustString(): String {
    return this.getString(0, RUST_STRING_ENCODING)
}
