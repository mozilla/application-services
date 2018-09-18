/* Copyright 2018 Mozilla
 * Licensed under the Apache License, Version 2.0 (the "License"); you may not use
 * this file except in compliance with the License. You may obtain a copy of the
 * License at http://www.apache.org/licenses/LICENSE-2.0
 * Unless required by applicable law or agreed to in writing, software distributed
 * under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
 * CONDITIONS OF ANY KIND, either express or implied. See the License for the
 * specific language governing permissions and limitations under the License. */
package org.mozilla.sync15.logins.rust

import com.sun.jna.Pointer
import com.sun.jna.Structure
import org.mozilla.sync15.logins.*
import java.util.Arrays

/**
 * This should be considered private, but it needs to be public for JNA.
 */
open class RustError : Structure() {

    class ByReference : RustError(), Structure.ByReference

    @JvmField var message: Pointer? = null
    @JvmField var code: Int = 0

    init {
        read()
    }

    /**
     * Does this represent success?
     */
    fun isSuccess(): Boolean {
        return code == 0;
    }

    /**
     * Does this represent failure?
     */
    fun isFailure(): Boolean {
        return code != 0;
    }

    fun intoException(): LoginsStorageException {
        if (!isFailure()) {
            // It's probably a bad idea to throw here! We're probably leaking something if this is
            // ever hit! (But we shouldn't ever hit it?)
            throw RuntimeException("[Bug] intoException called on non-failure!");
        }
        val message = this.consumeErrorMessage();
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
    fun consumeErrorMessage(): String {
        val result = this.getMessage()
        if (this.message != null) {
            PasswordSyncAdapter.INSTANCE.sync15_passwords_destroy_string(this.message!!);
            this.message = null
        }
        if (result == null) {
            throw NullPointerException("consumeErrorMessage called with null message!");
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
        return Arrays.asList("message", "code")
    }
}