/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.experiments.nimbus

import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.annotation.VisibleForTesting
import kotlinx.coroutines.runBlocking
import org.json.JSONException
import org.json.JSONObject

private const val NIMBUS_FLAG = "nimbus-cli"
private const val EXPERIMENTS_KEY = "experiments"
private const val LOG_STATE_KEY = "log-state"
private const val RESET_DB_KEY = "reset-db"
private const val EVAL_JEXL_KEY = "eval-jexl"
private const val VERSION_KEY = "version"
private const val DATA_KEY = "data"

/**
 * This method allows QA tooling to launch the app via an adb command-line,
 * and set up experiments at or just before first run.
 */
@Suppress("UNUSED_PARAMETER")
fun NimbusInterface.initializeTooling(context: Context, intent: Intent) {
    val args = createCommandLineArgs(intent) ?: return

    if (args.resetDatabase) {
        val job = resetEnrollmentsDatabase()
        runBlocking {
            job.join()
        }
    }

    args.experiments?.let { experiments ->
        runBlocking {
            applyLocalExperiments(experiments).join()
        }
        setFetchEnabled(false)
    }

    if (args.logState) {
        dumpStateToLog()
    }

    args.evalJexl?.let { expression ->
        // Evaluate JEXL and log the result
        @Suppress("TooGenericExceptionCaught")
        try {
            val messagingHelper = createMessageHelper()
            val resultJson = messagingHelper.evalJexlDebug(expression)

            android.util.Log.i("NimbusTooling", "JEXL_RESULT: $resultJson")
        } catch (e: Exception) {
            val errorJson = """{ "success": false, "errror": "${e.message}" }"""
            android.util.Log.e("NimbusTooling", "JEXL_RESULT: $errorJson")
        }
    }
}

@Suppress("ReturnCount")
@VisibleForTesting(otherwise = VisibleForTesting.PRIVATE)
internal fun createCommandLineArgs(intent: Intent): CliArgs? {
    if (!intent.hasExtra(NIMBUS_FLAG) || intent.getIntExtra(VERSION_KEY, 0) != 1) {
        return null
    }

    // There is a quoting within quoting problem meaning apostrophes don't get sent
    // through the multiple shells properly. This steps around this issue completely.
    val experiments = intent.getStringExtra(EXPERIMENTS_KEY)
        ?.replace("&apos;", "'")

    if (!validateExperiments(experiments)) {
        return null
    }

    val evalJexl = intent.getStringExtra(EVAL_JEXL_KEY)
        ?.replace("&apos;", "'")

    val resetDatabase = intent.getBooleanExtra(RESET_DB_KEY, false)
    val logState = intent.getBooleanExtra(LOG_STATE_KEY, false)

    return CliArgs(
        resetDatabase = resetDatabase,
        experiments = experiments,
        logState = logState,
        evalJexl = evalJexl,
    )
}

@Suppress("ReturnCount")
private fun validateExperiments(experiments: String?): Boolean {
    if (experiments == null) {
        return true
    }

    // We do some rudimentary taint checking of the string to make sure it looks
    // is a JSON object with a `data` key mapping to an array.
    try {
        val jsonObject = JSONObject(experiments)
        jsonObject.optJSONArray(DATA_KEY) ?: return false
    } catch (e: JSONException) {
        return false
    }

    return true
}

data class CliArgs(
    /** Whether to reset the enrollments database. */
    val resetDatabase: Boolean = false,
    /** The experiment payload (in legacy Remote Settings format) to apply. */
    val experiments: String? = null,
    /** Whether the list of currently active deliveries should be logged. */
    val logState: Boolean = false,
    /** A JEXL expression to evaluate. */
    val evalJexl: String? = null,
)
