/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.experiments.nimbus

import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.MainScope
import kotlinx.coroutines.asCoroutineDispatcher
import java.util.concurrent.Executors

typealias ErrorReporter = (message: String, e: Throwable) -> Unit
private typealias LoggerFunction = (message: String) -> Unit

/**
 * Provide calling apps control how Nimbus fits into it.
 */
class NimbusDelegate(
    /**
     * This is the coroutine scope that disk I/O occurs in, most notably the rkv database.
     */
    val dbScope: CoroutineScope,
    /**
     * This is the coroutine scope that the SDK talks to the network.
     */
    val fetchScope: CoroutineScope,
    /**
     * This is the coroutine scope that observers are notified on. By default, this is on the
     * {MainScope}. If this is `null`, then observers are notified on whichever thread the SDK
     * was called upon.
     */
    val updateScope: CoroutineScope? = MainScope(),
    val errorReporter: ErrorReporter,
    val logger: LoggerFunction,
) {
    companion object {
        private fun createCoroutineScope() = CoroutineScope(
            Executors.newSingleThreadExecutor().asCoroutineDispatcher(),
        )

        private const val logTag = "app-services-Nimbus.kt"

        fun default() = NimbusDelegate(
            dbScope = createCoroutineScope(),
            fetchScope = createCoroutineScope(),
            errorReporter = { msg: String, e: Throwable -> Log.e(logTag, msg, e) },
            logger = { Log.i(logTag, it) },
        )
    }
}
