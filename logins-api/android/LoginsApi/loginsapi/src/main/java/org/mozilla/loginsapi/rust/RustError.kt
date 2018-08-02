/* Copyright 2018 Mozilla
 * Licensed under the Apache License, Version 2.0 (the "License"); you may not use
 * this file except in compliance with the License. You may obtain a copy of the
 * License at http://www.apache.org/licenses/LICENSE-2.0
 * Unless required by applicable law or agreed to in writing, software distributed
 * under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
 * CONDITIONS OF ANY KIND, either express or implied. See the License for the
 * specific language governing permissions and limitations under the License. */
package org.mozilla.loginsapi.rust

import com.sun.jna.Pointer
import com.sun.jna.Structure
import java.util.Arrays

/**
 * This should be considered private, but it needs to be public for JNA.
 */
open class RustError : Structure() {

    class ByReference : RustError(), Structure.ByReference

    @JvmField var message: Pointer? = null

    init {
        read()
    }

    /**
     * Does this represent success?
     */
    fun isSuccess(): Boolean {
        return message == null;
    }

    /**
     * Does this represent failure?
     */
    fun isFailure(): Boolean {
        return message != null;
    }

    /**
     * Get and consume the error message, or null if there is none.
     */
    fun consumeErrorMessage(): String {
        val result = this.getMessage()
        if (this.message != null) {
            PasswordSyncAdapter.INSTANCE.destroy_c_char(this.message!!);
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
        return Arrays.asList("message")
    }
}