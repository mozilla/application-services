/* Copyright 2018 Mozilla
 * Licensed under the Apache License, Version 2.0 (the "License"); you may not use
 * this file except in compliance with the License. You may obtain a copy of the
 * License at http://www.apache.org/licenses/LICENSE-2.0
 * Unless required by applicable law or agreed to in writing, software distributed
 * under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
 * CONDITIONS OF ANY KIND, either express or implied. See the License for the
 * specific language governing permissions and limitations under the License. */
package org.mozilla.sync15.logins

import java.util.ArrayList

// TODO: Unfortunately, this is copied from fxa-result in mozilla-mobile (which AIUI is based on
// GeckoResult?). It would be nice to have a better way to avoid this issue that
// doesn't add a dependency on the fxa lib to the sync lib (or is that ok?)

/**
 * SyncResult is a class that represents an asynchronous result.
 *
 * @param <T> The type of the value delivered via the SyncResult.
 */
class SyncResult<T> {

    private enum class State {
        Pending,
        Success,
        Failure,
    }

    private var mState: State = State.Pending
    private var mValue: T? = null
    private var mError: Exception? = null

    private val mListeners: ArrayList<Listener<T>> = ArrayList()

    private interface Listener<T> {
        fun onValue(value: T)

        fun onException(exception: Exception)
    }

    /**
     * Completes this result based on another result.
     *
     * @param other The result that this result should mirror
     */
    private fun completeFrom(other: SyncResult<T>?) {
        if (other == null) {
            return
        }

        other.then(object : OnValueListener<T, Void> {
            override fun onValue(value: T): SyncResult<Void>? {
                complete(value)
                return null
            }
        }, object : OnExceptionListener<Void> {
            override fun onException(exception: Exception): SyncResult<Void>? {
                completeExceptionally(exception)
                return null
            }
        })
    }

    /**
     * Adds a value listener to be called when the [SyncResult] is completed with
     * a value. Listeners will be invoked on the same thread in which the
     * [SyncResult] was completed.
     *
     * @param fn A lambda expression with the same method signature as [OnValueListener],
     * called when the [SyncResult] is completed with a value.
     */
    fun <U> then(fn: (value: T) -> SyncResult<U>?): SyncResult<U> {
        val listener = object : OnValueListener<T, U> {
            override fun onValue(value: T): SyncResult<U>? = fn(value)
        }
        return then(listener, null)
    }

    /**
     * Adds listeners to be called when the [SyncResult] is completed either with
     * a value or [Exception]. Listeners will be invoked on the same thread in which the
     * [SyncResult] was completed.
     *
     * @param vfn A lambda expression with the same method signature as [OnValueListener],
     * called when the [SyncResult] is completed with a value.
     * @param efn A lambda expression with the same method signature as [OnExceptionListener],
     * called when the [SyncResult] is completed with an exception.
     */
    fun <U> then(vfn: (value: T) -> SyncResult<U>?, efn: (exception: Exception) -> SyncResult<U>?): SyncResult<U> {
        val valueListener = object : OnValueListener<T, U> {
            override fun onValue(value: T): SyncResult<U>? = vfn(value)
        }

        val exceptionListener = object : OnExceptionListener<U> {
            override fun onException(exception: Exception): SyncResult<U>? = efn(exception)
        }
        return then(valueListener, exceptionListener)
    }

    /**
     * Adds listeners to be called when the [SyncResult] is completed either with
     * a value or [Exception]. Listeners will be invoked on the same thread in which the
     * [SyncResult] was completed.
     *
     * @param valueListener An instance of [OnValueListener], called when the
     * [SyncResult] is completed with a value.
     * @param exceptionListener An instance of [OnExceptionListener], called when the
     * [SyncResult] is completed with an [Exception].
     */
    @Synchronized
    @Suppress("ComplexMethod", "TooGenericExceptionCaught")
    fun <U> then(valueListener: OnValueListener<T, U>, exceptionListener: OnExceptionListener<U>?): SyncResult<U> {
        val result = SyncResult<U>()
        val listener = object : Listener<T> {
            override fun onValue(value: T) {
                try {
                    result.completeFrom(valueListener.onValue(value))
                } catch (ex: Exception) {
                    result.completeFrom(fromException(ex))
                }
            }

            override fun onException(exception: Exception) {
                if (exceptionListener == null) {
                    // Do not swallow thrown exceptions if a listener is not present
                    throw exception
                }

                result.completeFrom(exceptionListener.onException(exception))
            }
        }

        // Note: This cast from `T?` to `T` can't be `mValue!!` because
        // `T` could be a nullable type.
        @Suppress("UNCHECKED_CAST")
        when (mState) {
            State.Success -> listener.onValue(mValue as T)
            State.Failure -> listener.onException(mError!!)
            State.Pending -> mListeners.add(listener)
        }

        return result
    }

    fun thenCatch(efn: (exception: Exception) -> SyncResult<T>): SyncResult<T> {
        return then({ SyncResult.fromValue(it) }, efn)
    }

    /**
     * Adds a value listener to be called when the [SyncResult] and the whole chain of [then]
     * calls is completed with a value. Listeners will be invoked on the same thread in
     * which the [SyncResult] was completed.
     *
     * @param fn A lambda expression with the same method signature as [OnValueListener],
     * called when the [SyncResult] is completed with a value.
     */
    fun whenComplete(fn: (value: T) -> Unit) {
        val listener = object : OnValueListener<T, Void> {
            override fun onValue(value: T): SyncResult<Void>? {
                fn(value)
                return SyncResult()
            }
        }
        then(listener, null)
    }

    /**
     * This completes the result with the specified value. IllegalStateException is thrown
     * if the result is already complete.
     *
     * @param value The value used to complete the result.
     * @throws IllegalStateException
     */
    @Synchronized
    fun complete(value: T) {
        if (mState != State.Pending) {
            throw IllegalStateException("result is already complete")
        }

        mValue = value
        mState = State.Success

        ArrayList(mListeners).forEach { it.onValue(value) }
    }

    /**
     * This completes the result with the specified [Exception]. IllegalStateException is thrown
     * if the result is already complete.
     *
     * @param exception The [Exception] used to complete the result.
     * @throws IllegalStateException
     */
    @Synchronized
    fun completeExceptionally(exception: Exception) {
        if (mState != State.Pending) {
            throw IllegalStateException("result is already complete")
        }

        mError = exception
        mState = State.Failure

        ArrayList(mListeners).forEach { it.onException(exception) }
    }

    /**
     * An interface used to deliver values to listeners of a [SyncResult]
     *
     * @param <T> This is the type of the value delivered via [.onValue]
     * @param <U> This is the type of the value for the result returned from [.onValue]
     */
    @FunctionalInterface
    interface OnValueListener<T, U> {
        /**
         * Called when a [SyncResult] is completed with a value. This will be
         * called on the same thread in which the result was completed.
         *
         * @param value The value of the [SyncResult]
         * @return A new [SyncResult], used for chaining results together.
         * May be null.
         */
        fun onValue(value: T): SyncResult<U>?
    }

    /**
     * An interface used to deliver exceptions to listeners of a [SyncResult]
     *
     * @param <V> This is the type of the vale for the result returned from [.onException]
     */
    @FunctionalInterface
    interface OnExceptionListener<V> {
        fun onException(exception: Exception): SyncResult<V>?
    }

    companion object {
        /**
         * This constructs a result that is fulfilled with the specified value.
         *
         * @param value The value used to complete the newly created result.
         * @return The completed [SyncResult]
         */
        fun <U> fromValue(value: U): SyncResult<U> {
            val result = SyncResult<U>()
            result.complete(value)
            return result
        }

        /**
         * This constructs a result that is completed with the specified [Exception].
         * May not be null.
         *
         * @param exception The exception used to complete the newly created result.
         * @return The completed [SyncResult]
         */
        fun <T> fromException(exception: Exception): SyncResult<T> {
            val result = SyncResult<T>()
            result.completeExceptionally(exception)
            return result
        }
    }
}
