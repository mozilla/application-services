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
private const val IS_LAUNCHER_KEY = "is-launcher"
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

    if (args.isLauncher) {
        intent.action = Intent.ACTION_MAIN
        intent.addCategory(Intent.CATEGORY_LAUNCHER)
    }
}

@Suppress("ReturnCount")
private fun createCommandLineArgs(intent: Intent): CliArgs? {
    // This incurs almost zero runtime cost in the release path.
    if (!intent.hasExtra(NIMBUS_FLAG)) {
        return intent.data?.let(::createCommandLineArgs)
    }

    if (intent.getIntExtra(VERSION_KEY, 0) != 1) {
        return null
    }

    val experiments = intent.getStringExtra(EXPERIMENTS_KEY)
        // There is a quoting within quoting problem meaning apostrophes don't get sent
        // through the multiple shells properly. This steps around this issue completely.
        ?.replace("&apos;", "'")

    val resetDatabase = intent.getBooleanExtra(RESET_DB_KEY, false)
    val logState = intent.getBooleanExtra(LOG_STATE_KEY, false)

    return check(CliArgs(resetDatabase, experiments, logState, false))
}

@Suppress("ReturnCount")
private fun check(args: CliArgs): CliArgs? {
    // We do some rudimentary taint checking of the string:
    // we make sure it looks like a JSON object, with a `data` key
    // and an array value.
    val string = args.experiments
    if (string != null) {
        try {
            val jsonObject = JSONObject(string)
            jsonObject.optJSONArray(DATA_KEY) ?: return null
        } catch (e: JSONException) {
            return null
        }
    }
    return args
}

@VisibleForTesting(otherwise = VisibleForTesting.PRIVATE)
@Suppress("ReturnCount")
fun createCommandLineArgs(uri: Uri): CliArgs? {
    if (!uri.isHierarchical || !uri.isAbsolute) {
        return null
    }
    if (setOf("http", "https").contains(uri.scheme ?: "")) {
        return null
    }
    val isMeantForUs = uri.getBooleanQueryParameter("--$NIMBUS_FLAG", false)
    if (!isMeantForUs) {
        return null
    }

    // Percent decoding happens transparently here:
    val experiments = uri.getQueryParameter("--$EXPERIMENTS_KEY")
    val resetDatabase = uri.getBooleanQueryParameter("--$RESET_DB_KEY", false)
    val logState = uri.getBooleanQueryParameter("--$LOG_STATE_KEY", false)
    val isLauncher = uri.getBooleanQueryParameter("--$IS_LAUNCHER_KEY", false)

    return check(CliArgs(resetDatabase, experiments, logState, isLauncher))
}

data class CliArgs(val resetDatabase: Boolean, val experiments: String?, val logState: Boolean, val isLauncher: Boolean)
