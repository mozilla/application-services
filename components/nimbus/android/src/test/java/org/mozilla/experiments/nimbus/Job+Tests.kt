package org.mozilla.experiments.nimbus

import kotlinx.coroutines.*
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

@RunWith(RobolectricTestRunner::class)
class JobTests {

    @Test
    fun `joinOrTimeout is true if the job completes within the time limit`() {
        runBlocking {
            launch {
                var x = 0;
                while (isActive && x < 10) {
                    println("waiting...")
                    delay(10)
                    x++
                }
            }.also { job ->
                assertEquals(true, job.joinOrTimeout(150L))
            }
        }
    }

    @Test
    fun `joinOrTimeout is false if the job does not complete within the time limit`() {
        runBlocking {
            launch {
                var x = 0;
                while (isActive && x < 10) {
                    println("waiting...")
                    delay(10)
                    x++
                }
            }.also { job ->
                assertEquals(false, job.joinOrTimeout(90L))
            }
        }
    }

    @Test
    fun `joinOrTimeout is false if job is already cancelled`() {
        runBlocking {
            launch {
                var x = 0;
                while (isActive && x < 10) {
                    println("waiting...")
                    delay(10)
                    x++
                }
            }.also { job ->
                job.cancel()
                assertEquals(false, job.joinOrTimeout(0L))
            }
        }
    }

    @Test
    fun `joinOrTimeout is true if job is already completed`() {
        runBlocking {
            launch {

            }.also { job ->
                job.join()
                assertEquals(true, job.joinOrTimeout(0L))
            }
        }
    }
}
