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