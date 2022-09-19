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
        e.printStackTrace()
        try {
            cancelAndJoin()
        } catch (e: Exception) {
            e.printStackTrace()
        }
        false
    }
