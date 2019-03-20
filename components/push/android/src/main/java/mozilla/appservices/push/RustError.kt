/* Copyright 2018 Mozilla
 * Licensed under the Apache License, Version 2.0 (the "License"); you may not use
 * this file except in compliance with the License. You may obtain a copy of the
 * License at http://www.apache.org/licenses/LICENSE-2.0
 * Unless required by applicable law or agreed to in writing, software distributed
 * under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
 * CONDITIONS OF ANY KIND, either express or implied. See the License for the
 * specific language governing permissions and limitations under the License. */
package mozilla.appservices.push

import com.sun.jna.Pointer
import com.sun.jna.Structure
import java.util.Arrays

// Mirror the Rust errors from push/error/lib.rs
open class PushError(msg: String) : Exception(msg)
open class InternalPanic(msg: String) : PushError(msg)
open class OpenSSLError(msg: String) : PushError(msg)
open class CommunicationError(msg: String) : PushError(msg)
open class CommunicationServerError(msg: String) : PushError(msg)
open class AlreadyRegisteredError : PushError(
        "This channelID is already registered.")
open class StorageError(msg: String) : PushError(msg)
open class MissingRegistrationTokenError : PushError(
        "Missing Registration Token. Please register with OS first.")
open class StorageSqlError(msg: String) : PushError(msg)
open class TranscodingError(msg: String) : PushError(msg)
open class EncryptionError(msg: String) : PushError(msg)

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
     * Does this represent success?
     */
    fun isSuccess(): Boolean {
        return code == 0
    }

    /**
     * Does this represent failure?
     */
    fun isFailure(): Boolean {
        return code != 0
    }

    fun intoException(): PushError {
        if (!isFailure()) {
            // It's probably a bad idea to throw here! We're probably leaking something if this is
            // ever hit! (But we shouldn't ever hit it?)
            throw RuntimeException("[Bug] intoException called on non-failure!")
        }
        val message = this.consumeErrorMessage()
        when (code) {
            24 -> return OpenSSLError(message)
            25 -> return CommunicationError(message)
            26 -> return CommunicationServerError(message)
            27 -> return AlreadyRegisteredError()
            28 -> return StorageError(message)
            29 -> return StorageSqlError(message)
            30 -> return MissingRegistrationTokenError()
            31 -> return TranscodingError(message)
            32 -> return EncryptionError(message)
            -1 -> return InternalPanic(message)
            // Note: `1` is used as a generic catch all, but we
            // might as well handle the others the same way.
            else -> return PushError(message)
        }
    }

    /**
     * Get and consume the error message, or null if there is none.
     */
    fun consumeErrorMessage(): String {
        val result = this.getMessage()
        if (this.message != null) {
            LibPushFFI.INSTANCE.push_destroy_string(this.message!!)
            this.message = null
        }
        if (result == null) {
            throw NullPointerException("consumeErrorMessage called with null message!")
        }
        return result
    }

    /**
     * Get the error message or null if there is none.
     */
    fun getMessage(): String? {
        return this.message?.getString(0, "utf8")
    }

    override fun getFieldOrder(): List<String> {
        return Arrays.asList("code", "message")
    }
}
