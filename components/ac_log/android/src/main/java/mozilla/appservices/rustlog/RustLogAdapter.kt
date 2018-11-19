/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.rustlog

import com.sun.jna.Pointer

typealias OnLog = (Int, String?, String) -> Unit;
class RustLogAdapter private constructor(
    // IMPORTANT: This must not be GCed while the adapter is alive!
        @Suppress("Unused")
        private val callbackImpl: RawLogCallbackImpl,
        private val adapter: RawLogAdapter
) {
    companion object {
        @Volatile
        private var instance: RustLogAdapter? = null

        @Volatile
        private var everEnabled: Boolean = false

        /**
         * true if the log is enabled.
         */
        val isEnabled get() = instance != null

        /**
         * True if the log can be enabled.
         *
         * Note that this isn't the same as `!isEnabled`, as the log
         * cannot be re-enabled after it is disabled.
         */
        val canEnable get() = !everEnabled

        /**
         * Enable the logger and use the provided logging callback.
         *
         * Note that the logger can only be enabled once.
         */
        @Synchronized
        fun enable(onLog: OnLog) {
            val wasEnabled = everEnabled
            everEnabled = true
            if (wasEnabled) {
                // This is mostly for debugging. It really shouldn't happen at runtime.
                val message = if (isEnabled) {
                    "Adapter is already enabled"
                } else {
                    "Adapter has previously been disabled"
                }
                throw LogAdapterCannotEnable(message)
            }
            val callbackImpl = RawLogCallbackImpl(onLog)
            // Hopefully there is no way to half-initialize the logger such that where the callback
            // could still get called despite an error/null being returned? If there is, we need to
            // make callbackImpl isn't GCed here, or very bad things will happen. (Should the logger
            // init code abort on panic?)
            val adapter = rustCall { err ->
                LibRustLogAdapter.INSTANCE.ac_log_adapter_create(callbackImpl, err)
            }
            // For example, it would be *extremely bad* if somehow adapter were actually null here.
            instance = RustLogAdapter(callbackImpl, adapter!!)
        }

        /**
         * Helper to enable the logger if it can be enabled. Returns true if
         * the logger was enabled by this call.
         */
        @Synchronized
        fun tryEnable(onLog: OnLog): Boolean {
            if (!canEnable) {
                return false
            }
            enable(onLog)
            return true
        }

        /**
         * Disable the logger, allowing the logging callback to be garbage collected.
         *
         * Note that the logger can only be enabled once.
         */
        @Synchronized
        fun disable() {
            val state = instance ?: return
            LibRustLogAdapter.INSTANCE.ac_log_adapter_destroy(state.adapter)
            // XXX Letting that callback get GCed still makes me extremely uneasy...
            // Maybe we should just null out the callback provided by the user so that
            // it can be GCed (while letting the RawLogCallbackImpl which actually is
            // called by Rust live on).
            instance = null
        }

        @Synchronized
        fun setMaxLevel(level: LogLevelFilter) {
            if (isEnabled) {
                rustCall { e ->
                    LibRustLogAdapter.INSTANCE.ac_log_adapter_set_max_level(
                            instance!!.adapter,
                            level.value,
                            e
                    )
                }
            }
        }

        private inline fun <U> rustCall(callback: (RustError.ByReference) -> U): U {
            val e = RustError.ByReference()
            val ret: U = callback(e)
            if (e.isFailure()) {
                val msg = e.consumeErrorMessage()
                throw LogAdapterUnexpectedError(msg)
            } else {
                return ret
            }
        }
    }
}

/**
 * All errors emitted by the LogAdapter will subclass this.
 */
sealed class LogAdapterError(msg: String) : Exception(msg)

/**
 * Error indicating that the log adapter cannot be enabled.
 *
 * The log adapter may only be enabled once, and once it is disabled,
 * it may never be enabled again.
 *
 * This is, admittedly, inconvenient, and future versions of library may work
 * around this limitation in Rust's default log system.
 */
class LogAdapterCannotEnable(msg: String) : LogAdapterError("Log adapter may not be enabled: $msg")

/**
 * Thrown for unexpected log adapter errors (generally rust panics).
 */
class LogAdapterUnexpectedError(msg: String): LogAdapterError("Unexpected log adapter error: $msg")

// Note: keep values in sync with level_filter_from_i32 in rust.
/** Level filters, for use with setMaxLevel. */
enum class LogLevelFilter(internal val value: Int) {
    /** Disable all logging */
    OFF(0),
    /** Only allow ERROR logs. */
    ERROR(1),
    /** Allow WARN and ERROR logs. */
    WARN(2),
    /** Allow WARN, ERROR, and INFO logs. The default. */
    INFO(3),
    /** Allow WARN, ERROR, INFO, and DEBUG logs. */
    DEBUG(4),
    /** Allow all logs, including those that may contain PII. */
    TRACE(5),
}


internal class RawLogCallbackImpl(private val onLog: OnLog) : RawLogCallback {
    override fun invoke(level: Int, tag: Pointer?, message: Pointer) {
        // We can't safely throw here!
        try {
            val tagStr = tag?.getString(0, "utf8")
            val msgStr = message.getString(0, "utf8")
            onLog(level, tagStr, msgStr)
        } catch(e: Throwable) {
            try {
                println("Exception when logging: $e")
            } catch (e: Throwable) {
                // :(
            }
        }
    }
}

internal fun Pointer.getAndConsumeRustString(): String {
    try {
        return this.getString(0, "utf8")
    } finally {
        LibRustLogAdapter.INSTANCE.ac_log_adapter_destroy_string(this)
    }
}
