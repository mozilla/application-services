/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.experiments.nimbus

import android.content.Context
import android.net.Uri
import androidx.annotation.RawRes
import kotlinx.coroutines.runBlocking
import java.util.Locale

private const val TIME_OUT_LOADING_EXPERIMENT_FROM_DISK_MS = 200L

/**
 * A builder for [Nimbus] singleton objects, parameterized in a declarative class.
 */
abstract class AbstractNimbusBuilder<T : NimbusInterface>(val context: Context) {

    /**
     * An optional server URL string.
     *
     * This will only be null or empty in development or testing, or in any build variant of a
     * non-Mozilla fork.
     */
    var url: String? = null

    /**
     * A closure for reporting errors from Rust.
     */
    var errorReporter: ErrorReporter = { _: String, _: Throwable -> }

    /**
     * A flag to select the main or preview collection of remote settings. Defaults to `false`.
     */
    var usePreviewCollection: Boolean = false

    /**
     * A flag to indicate if this is being run on the first run of the app. This is used to control
     * whether the `initial_experiments` file is used to populate Nimbus.
     */
    var isFirstRun: Boolean = true

    /**
     * A optional raw resource of a file downloaded at or near build time from Remote Settings.
     */
    @RawRes
    var initialExperiments: Int? = null

    /**
     * The timeout used to wait for the loading of the `initial_experiments`.
     */
    var timeoutLoadingExperiment: Long = TIME_OUT_LOADING_EXPERIMENT_FROM_DISK_MS

    /**
     * Optional callback to be called after the creation of the nimbus object and it is ready
     * to be used.
     */
    var onCreateCallback: (T) -> Unit = {}

    /**
     * Build a [Nimbus] singleton for the given [NimbusAppInfo]. Instances built with this method
     * have been initialized, and are ready for use by the app.
     *
     * Instance have _not_ yet had [fetchExperiments()] called on it, or anything usage of the
     * network. This is to allow the networking stack to be initialized after this method is called
     * and the networking stack to be involved in experiments.
     */
    fun build(appInfo: NimbusAppInfo): T {
        // Eventually we'll want to use `NimbusDisabled` when we have no NIMBUS_ENDPOINT.
        // but we keep this here to not mix feature flags and how we configure Nimbus.
        val serverSettings: NimbusServerSettings? = if (!url.isNullOrBlank()) {
            if (usePreviewCollection) {
                NimbusServerSettings(url = Uri.parse(url), collection = "nimbus-preview")
            } else {
                NimbusServerSettings(url = Uri.parse(url))
            }
        } else {
            null
        }

        @Suppress("TooGenericExceptionCaught")
        return try {
            newNimbus(appInfo, serverSettings).apply {
                // Apply any experiment recipes we downloaded last time, or
                // if this is the first time, we load the ones bundled in the res/raw
                // directory.
                val job = if ((isFirstRun || url.isNullOrBlank()) && initialExperiments != null) {
                    applyLocalExperiments(initialExperiments!!)
                } else {
                    applyPendingExperiments()
                }

                // We always want initialize Nimbus to happen ASAP and before any features (engine/UI)
                // have been initialized. For that reason, we use runBlocking here to avoid
                // inconsistency in the experiments.
                // We can safely do this because Nimbus does most of its work on background threads,
                // including the loading the initial experiments from disk. For this reason, we have a
                // `joinOrTimeout` to limit the blocking until `timeoutLoadingExperiment`.
                runBlocking {
                    // We only read from disk when loading first-run experiments. This is the only time
                    // that we should join and block. Otherwise, we don't want to wait.
                    job.joinOrTimeout(timeoutLoadingExperiment)
                }
                // By now, on this thread, we have a fully initialized Nimbus object, ready for use:
                // * we gave a 200ms timeout to the loading of a file from res/raw
                // * on completion or cancellation, applyPendingExperiments or initialize was
                //   called, and this thread waited for that to complete.
                onCreateCallback(this)
            }
        } catch (e: Throwable) {
            // Something went wrong. We'd like not to, but stability of the app is more important than
            // failing fast here.
            errorReporter("Failed to initialize Nimbus", e)
            newNimbusDisabled()
        }
    }

    /**
     * Construct a new [NimbusInterface] object with the passed parameters.
     */
    protected abstract fun newNimbus(
        appInfo: NimbusAppInfo,
        serverSettings: NimbusServerSettings?
    ): T

    /**
     * In the event of the error constructing or configuring a Rust backed
     * [NimbusInterface] object, then construct a dummy object.
     */
    protected abstract fun newNimbusDisabled(): T
}

class DefaultNimbusBuilder(context: Context) : AbstractNimbusBuilder<NimbusInterface>(context) {
    override fun newNimbus(appInfo: NimbusAppInfo, serverSettings: NimbusServerSettings?) =
        Nimbus(context,
            appInfo = appInfo,
            server = serverSettings,
            deviceInfo = NimbusDeviceInfo(Locale.getDefault().toLanguageTag()),
            delegate = NimbusDelegate.default()
        )

    override fun newNimbusDisabled() = NullNimbus(context)
}
