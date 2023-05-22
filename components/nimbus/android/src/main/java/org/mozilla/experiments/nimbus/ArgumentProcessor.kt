/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.experiments.nimbus

import android.content.Context
import android.content.Intent
import kotlinx.coroutines.runBlocking
import org.json.JSONException
import org.json.JSONObject

private const val NIMBUS_FLAG = "nimbus-cli"
private const val EXPERIMENTS_KEY = "experiments"
private const val LOG_STATE_KEY = "log-state"
private const val RESET_DB_KEY = "reset-db"
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
        setExperimentsLocally(experiments)
        val job = applyPendingExperiments()
        runBlocking {
            job.join()
        }
        setFetchEnabled(false)
    }

    if (args.logState) {
        dumpStateToLog()
    }
}

@Suppress("ReturnCount")
private fun createCommandLineArgs(intent: Intent): CliArgs? {
    // This incurs almost zero runtime cost in the release path.
    if (!intent.hasExtra(NIMBUS_FLAG)) {
        return null
    }

    if (intent.getIntExtra(VERSION_KEY, 0) != 1) {
        return null
    }

    val experiments = intent.getStringExtra(EXPERIMENTS_KEY)
        // There is a quoting within quoting problem meaning apostrophes don't get sent
        // through the multiple shells properly. This steps around this issue completely.
        ?.replace("&apos;", "'")
        ?.let { string ->
            // We do some rudimentary taint checking of the string:
            // we make sure it looks like a JSON object, with a `data` key
            // and an array value.
            try {
                val jsonObject = JSONObject(string)
                jsonObject.optJSONArray(DATA_KEY) ?: return@let null
                string
            } catch (e: JSONException) {
                null
            }
        }

    val resetDatabase = intent.getBooleanExtra(RESET_DB_KEY, false)
    val logState = intent.getBooleanExtra(LOG_STATE_KEY, false)

    return CliArgs(resetDatabase, experiments, logState)
}

data class CliArgs(val resetDatabase: Boolean, val experiments: String?, val logState: Boolean)
