/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.logins.rust

import com.sun.jna.Pointer
import com.sun.jna.Structure
import mozilla.appservices.logins.SyncAuthInvalidException
import mozilla.appservices.logins.NoSuchRecordException
import mozilla.appservices.logins.IdCollisionException
import mozilla.appservices.logins.InvalidRecordException
import mozilla.appservices.logins.InvalidKeyException
import mozilla.appservices.logins.RequestFailedException
import mozilla.appservices.logins.LoginsStorageException
import mozilla.appservices.logins.getAndConsumeRustString
import mozilla.appservices.logins.getRustString
import java.util.Arrays

/**
 * This should be considered private, but it needs to be public for JNA.
 */
open class RustError : Structure() {

    class ByReference : RustError(), Structure.ByReference

    @JvmField var code: Int = 0
    @JvmField var message: Pointer? = null

    init {
        read()
    }

    /**
     * Does this represent failure?
     */
    fun isFailure(): Boolean {
        return code != 0
    }

    @Suppress("ReturnCount", "TooGenericExceptionThrown")
    fun intoException(): LoginsStorageException {
        if (!isFailure()) {
            // It's probably a bad idea to throw here! We're probably leaking something if this is
            // ever hit! (But we shouldn't ever hit it?)
            throw RuntimeException("[Bug] intoException called on non-failure!")
        }
        val message = this.consumeErrorMessage()
        when (code) {
            1 -> return SyncAuthInvalidException(message)
            2 -> return NoSuchRecordException(message)
            3 -> return IdCollisionException(message)
            4 -> return InvalidRecordException(message)
            5 -> return InvalidKeyException(message)
            6 -> return RequestFailedException(message)
            else -> return LoginsStorageException(message)
        }
    }

    /**
     * Get and consume the error message, or null if there is none.
     */
    @Synchronized
    fun consumeErrorMessage(): String {
        val result = this.message?.getAndConsumeRustString()
        this.message = null
        if (result == null) {
            throw NullPointerException("consumeErrorMessage called with null message!")
        }
        return result
    }

    @Synchronized
    fun ensureConsumed() {
        this.message?.getAndConsumeRustString()
        this.message = null
    }

    /**
     * Get the error message or null if there is none.
     */
    fun getMessage(): String? {
        return this.message?.getRustString()
    }

    override fun getFieldOrder(): List<String> {
        return Arrays.asList("code", "message")
    }
}
