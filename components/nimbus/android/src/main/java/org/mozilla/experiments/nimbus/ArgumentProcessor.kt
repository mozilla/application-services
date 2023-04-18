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
private const val VERSION_KEY = "version"
private const val DATA_KEY = "data"

/**
 * This method allows QA tooling to launch the app via an adb command-line,
 * and set up experiments at or just before first run.
 */
@Suppress("UNUSED_PARAMETER", "ReturnCount")
fun NimbusInterface.initializeTooling(context: Context, intent: Intent) {
    if (!intent.hasExtra(NIMBUS_FLAG)) {
        return
    }

    if (intent.getIntExtra(VERSION_KEY, 0) != 1) {
        return
    }

    val experiments = intent.getStringExtra(EXPERIMENTS_KEY) ?: return

    // We do some rudimentary taint checking of the string:
    // we make sure it looks like a JSON object, with a `data` key
    // and an array value.
    try {
        val jsonObject = JSONObject(experiments)
        jsonObject.optJSONArray(DATA_KEY) ?: return
    } catch (e: JSONException) {
        return
    }

    setExperimentsLocally(experiments)
    val job = applyPendingExperiments()
    runBlocking {
        job.join()
    }

    setFetchEnabled(false)
}
