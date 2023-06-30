/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

@file:Suppress("ktlint:standard:filename")

package org.mozilla.experiments.nimbus

import kotlinx.coroutines.Job
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.withTimeout

/**
 * Utility function to wait for the end of job. If the given timeout is reached then
 * the job is cancelled.
 */
suspend fun Job.joinOrTimeout(timeout: Long): Boolean =
    try {
        if (isCancelled) {
            false
        } else if (isCompleted) {
            true
        } else {
            withTimeout(timeout) {
                join()
                true
            }
        }
    } catch (e: TimeoutCancellationException) {
        // We are not cancelled, nor completed.
        if (isActive) {
            cancelAndJoin()
        }
        false
    }
