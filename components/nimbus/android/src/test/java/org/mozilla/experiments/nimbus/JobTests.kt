package org.mozilla.experiments.nimbus

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import java.util.concurrent.Executors

@RunWith(RobolectricTestRunner::class)
class JobTests {
    @Test
    fun `joinOrTimeout is true if the job completes within the time limit`() {
        runBlocking {
            launch {
                var x = 0
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
                var x = 0
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
                var x = 0
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
                // NOOP
            }.also { job ->
                job.join()
                assertEquals(true, job.joinOrTimeout(0L))
            }
        }
    }

    @Test
    fun `job can timeout`() {
        var completed = false
        val job = dbScope().launch {
            delay(1000)
            completed = true
        }
        val finished = runBlocking {
            job.joinOrTimeout(250L)
        }

        Assert.assertFalse(completed)
        assertEquals(completed, finished)
    }

    @Test
    fun `job can complete`() {
        var completed = false
        val job = dbScope().launch {
            delay(250)
            completed = true
        }
        val finished = runBlocking {
            job.joinOrTimeout(1000L)
        }

        Assert.assertTrue(completed)
        assertEquals(completed, finished)
    }

    @Test
    fun `completed job is shown as complete`() {
        var completed = false
        val job = dbScope().launch {
            completed = true
        }
        runBlocking {
            delay(250)
        }
        val finished = runBlocking {
            job.joinOrTimeout(1000L)
        }

        Assert.assertTrue(completed)
        assertEquals(completed, finished)
    }

    private fun dbScope(): CoroutineScope =
        CoroutineScope(Executors.newSingleThreadExecutor().asCoroutineDispatcher())
}
