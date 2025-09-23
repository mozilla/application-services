/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.experiments.nimbus

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import kotlinx.coroutines.Job
import mozilla.appservices.remotesettings.RemoteSettingsConfig2
import mozilla.appservices.remotesettings.RemoteSettingsService
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import kotlin.random.Random

@RunWith(RobolectricTestRunner::class)
class NimbusBuilderTest {
    private val context: Context
        get() = ApplicationProvider.getApplicationContext()

    private val appInfo = NimbusAppInfo(
        appName = "test-app",
        channel = "test-channel",
    )

    @Test
    fun `test use preview collection`() {
        val n1 = NimbusBuilder(context).apply {
        }.build(
            appInfo,
            NimbusServerSettings(
                remoteSettingsService = RemoteSettingsService(storageDir = "dummy", config = RemoteSettingsConfig2()),
                collection = "nimbus-preview",
            ),
        ) as DummyNimbus
        assertTrue(n1.usePreviewCollection)

        val n2 = NimbusBuilder(context).apply {
            url = "https://example.com"
        }.build(appInfo, null) as DummyNimbus
        assertFalse(n2.usePreviewCollection)

        // Without a URL, there is no preview collection
        val n3 = NimbusBuilder(context).apply {
        }.build(appInfo, null) as DummyNimbus
        assertFalse(n3.usePreviewCollection)
    }

    @Test
    fun `test use bundled experiments on first run only`() {
        val bundledExperiments = Random.nextInt()

        val n0 = NimbusBuilder(context).build(appInfo, null) as DummyNimbus
        assertNull(n0.initialExperiments)

        // Normal operation, first run.
        val normalFirstRun = NimbusBuilder(context).apply {
            url = "https://example.com"
            isFirstRun = true
            initialExperiments = bundledExperiments
        }.build(appInfo, null) as DummyNimbus
        assertEquals(bundledExperiments, normalFirstRun.initialExperiments)

        // Normal operation, subsequent runs
        val normalNonFirstRun = NimbusBuilder(context).apply {
            url = "https://example.com"
            isFirstRun = false
            initialExperiments = bundledExperiments
        }.build(appInfo, null) as DummyNimbus
        assertNull(normalNonFirstRun.initialExperiments)

        // Normal operation, without bundling
        val fetchOnFirstRun = NimbusBuilder(context).apply {
            url = "https://example.com"
            isFirstRun = false
        }.build(appInfo, null) as DummyNimbus
        assertNull(fetchOnFirstRun.initialExperiments)

        // Local development operation, first run
        val devBuild1 = NimbusBuilder(context).apply {
            isFirstRun = true
            initialExperiments = bundledExperiments
        }.build(appInfo, null) as DummyNimbus
        assertEquals(bundledExperiments, devBuild1.initialExperiments)

        // Local development operation, subsequent
        val devBuild2 = NimbusBuilder(context).apply {
            isFirstRun = false
            initialExperiments = bundledExperiments
        }.build(appInfo, null) as DummyNimbus
        assertEquals(bundledExperiments, devBuild2.initialExperiments)
    }

    @Test
    fun `test use not used when nimbus-cli is in use`() {
        val bundledExperiments = Random.nextInt()
        // Local development operation, first run
        val devBuild1 = NimbusBuilder(context).apply {
            url = null
            isFirstRun = true
            initialExperiments = bundledExperiments
        }.build(appInfo, null) as DummyNimbus
        assertEquals(bundledExperiments, devBuild1.initialExperiments)

        // Local development operation, subsequent runs, but with isFetchEnabled = false
        // Note that isFetchEnabled is part of the testing framework, passed directly to DummyNimbus
        // then checked by the NimbusBuilder to work out whether to apply the local initial_experiments.
        val devBuild2 = NimbusBuilder(context, isFetchEnabled = false).apply {
            url = null
            isFirstRun = false
            initialExperiments = bundledExperiments
        }.build(appInfo, null) as DummyNimbus
        assertNull(devBuild2.initialExperiments)
    }
}

class NimbusBuilder(
    context: Context,
    val isFetchEnabled: Boolean = true,
) : AbstractNimbusBuilder<NimbusInterface>(context) {
    override fun newNimbus(
        appInfo: NimbusAppInfo,
        serverSettings: NimbusServerSettings?,
    ): NimbusInterface =
        DummyNimbus(context, appInfo = appInfo, serverSettings = serverSettings, isFetchEnabled = isFetchEnabled)

    override fun newNimbusDisabled(): NimbusInterface =
        NullNimbus(context)
}

class DummyNimbus(
    override val context: Context,
    val serverSettings: NimbusServerSettings?,
    val appInfo: NimbusAppInfo,
    private val isFetchEnabled: Boolean,
) : NimbusInterface {

    var initialExperiments: Int? = null

    val usePreviewCollection: Boolean
        get() = serverSettings?.collection == "nimbus-preview"

    override fun applyLocalExperiments(file: Int): Job {
        initialExperiments = file
        return super.applyLocalExperiments(file)
    }

    override fun isFetchEnabled(): Boolean {
        return isFetchEnabled
    }
}
