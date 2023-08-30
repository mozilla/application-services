/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.experiments.nimbus

import android.net.Uri
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import java.net.URLEncoder

@RunWith(RobolectricTestRunner::class)
class ArgumentProcessorTest {
    fun `test createCliArgsFromUri flags`() {
        val obs = createCommandLineArgs(
            Uri.parse("my-app://foo?--nimbus-cli"),
        )
        assertNotNull(obs)
        assertEquals(CliArgs(false, null, false, false), obs)

        val obs1 = createCommandLineArgs(
            Uri.parse("my-app://foo?--nimbus-cli&--reset-db"),
        )
        assertEquals(CliArgs(true, null, false, false), obs1)

        val obs2 = createCommandLineArgs(
            Uri.parse("my-app://foo?--reset-db&--nimbus-cli&--log-state"),
        )
        assertEquals(CliArgs(true, null, true, false), obs2)

        val obs3 = createCommandLineArgs(
            Uri.parse("my-app://foo?--nimbus-cli=true&--reset-db=1&--log-state"),
        )
        assertEquals(CliArgs(true, null, true, false), obs3)

        val obs4 = createCommandLineArgs(
            Uri.parse("my-app://foo?--nimbus-cli&--reset-db=0&--log-state=false"),
        )
        assertEquals(CliArgs(false, null, false, false), obs4)
    }

    @Test
    fun `test createCliArgsFromUri experiments JSON`() {
        val unenrollAll = "{\"data\":[]}"
        val obs = createCommandLineArgs(
            Uri.parse("my-app://foo?--nimbus-cli&--experiments=$unenrollAll"),
        )
        assertNotNull(obs)
        assertEquals(CliArgs(false, unenrollAll, false, false), obs)

        val encoded = URLEncoder.encode(unenrollAll, "UTF-8")
        assertNotEquals(encoded, unenrollAll)

        val obs1 = createCommandLineArgs(
            Uri.parse("my-app://foo?--nimbus-cli&--experiments=$encoded"),
        )
        assertNotNull(obs1)
        assertEquals(CliArgs(false, unenrollAll, false, false), obs1)
    }

    @Test
    fun `test createCliArgsFromUri experiments JSON sanity check`() {
        val good = "{\"data\":[]}"
        val obs = createCommandLineArgs(
            Uri.parse("my-app://foo?--nimbus-cli&--experiments=$good"),
        )
        assertNotNull(obs)
        assertEquals(CliArgs(false, good, false, false), obs)

        fun isInvalid(bad: String) {
            val obs0 = createCommandLineArgs(
                Uri.parse("my-app://foo?--nimbus-cli&--experiments=$bad"),
            )
            assertNull(obs0)
        }

        isInvalid("{}")
        isInvalid("[]")
        isInvalid("{\"data\": 1}")
    }

    @Test
    fun `test createCliArgsFromUri with badly formed URL safely fails`() {
        val experiments = "{\"data\":[]}"
        val encoded = URLEncoder.encode(experiments, "UTF-8")
        fun isNotForNimbus(bad: String) {
            val obs0 = createCommandLineArgs(
                Uri.parse("$bad?--nimbus-cli&--experiments=$encoded"),
            )
            assertNull(obs0)
        }

        isNotForNimbus("")
        isNotForNimbus("host")
        isNotForNimbus("host.tld")
        isNotForNimbus("mailto:foo")
        isNotForNimbus("mailto:me@there.com")

        isNotForNimbus("https://example.com/webpage")
    }

    @Test
    fun `test createCliArgsFromUri with normal URLs safely fails`() {
        fun isNotForNimbus(bad: String) {
            val obs0 = createCommandLineArgs(
                Uri.parse(bad),
            )
            assertNull(obs0)
        }

        isNotForNimbus("")
        isNotForNimbus("host")
        isNotForNimbus("host.tld")
        isNotForNimbus("mailto:foo")
        isNotForNimbus("mailto:me@there.com")

        isNotForNimbus("https://example.com/webpage")
    }

    @Test
    fun `test from Rust`() {
        val url = "fenix-dev://open?--nimbus-cli&--experiments=%7B%22data%22%3A[%7B%22appId%22%3A%22org.mozilla.firefox%22,%22appName%22%3A%22fenix%22,%22application%22%3A%22org.mozilla.firefox%22,%22arguments%22%3A%7B%7D,%22branches%22%3A[%7B%22feature%22%3A%7B%22enabled%22%3Afalse,%22featureId%22%3A%22this-is-included-for-mobile-pre-96-support%22,%22value%22%3A%7B%7D%7D,%22features%22%3A[%7B%22enabled%22%3Atrue,%22featureId%22%3A%22juno-onboarding%22,%22value%22%3A%7B%22enabled%22%3Atrue%7D%7D],%22ratio%22%3A0,%22slug%22%3A%22control%22%7D,%7B%22feature%22%3A%7B%22enabled%22%3Afalse,%22featureId%22%3A%22this-is-included-for-mobile-pre-96-support%22,%22value%22%3A%7B%7D%7D,%22features%22%3A[%7B%22enabled%22%3Atrue,%22featureId%22%3A%22juno-onboarding%22,%22value%22%3A%7B%22cards%22%3A%7B%22default-browser%22%3A%7B%22body%22%3A%22Nimm%20nicht%20das%20Erstbeste,%20sondern%20das%20Beste%20f%C3%BCr%20dich%3A%20Firefox%20sch%C3%BCtzt%20deine%20Privatsph%C3%A4re.\n\nLies%20unseren%20Datenschutzhinweis.%22,%22image-res%22%3A%22onboarding_ctd_default_browser%22,%22link-text%22%3A%22Datenschutzhinweis%22,%22title%22%3A%22Du%20entscheidest,%20was%20Standard%20ist%22%7D,%22notification-permission%22%3A%7B%22body%22%3A%22Benachrichtigungen%20helfen%20dabei,%20Downloads%20zu%20managen%20und%20Tabs%20zwischen%20Ger%C3%A4ten%20zu%20senden.%22,%22image-res%22%3A%22onboarding_ctd_notification%22,%22title%22%3A%22Du%20bestimmst,%20was%20Firefox%20kann%22%7D,%22sync-sign-in%22%3A%7B%22body%22%3A%22Wenn%20du%20willst,%20bringt%20Firefox%20deine%20Tabs%20und%20Passw%C3%B6rter%20auf%20all%20deine%20Ger%C3%A4te.%22,%22image-res%22%3A%22onboarding_ctd_sync%22,%22title%22%3A%22Alles%20ist%20dort,%20wo%20du%20es%20brauchst%22%7D%7D,%22enabled%22%3Atrue%7D%7D],%22ratio%22%3A100,%22slug%22%3A%22treatment-a%22%7D],%22bucketConfig%22%3A%7B%22count%22%3A10000,%22namespace%22%3A%22fenix-juno-onboarding-release-3%22,%22randomizationUnit%22%3A%22nimbus_id%22,%22start%22%3A0,%22total%22%3A10000%7D,%22channel%22%3A%22developer%22,%22endDate%22%3Anull,%22enrollmentEndDate%22%3A%222023-07-18%22,%22featureIds%22%3A[%22juno-onboarding%22],%22featureValidationOptOut%22%3Afalse,%22id%22%3A%22on-boarding-challenge-the-default%22,%22isEnrollmentPaused%22%3Afalse,%22isRollout%22%3Afalse,%22locales%22%3Anull,%22localizations%22%3Anull,%22outcomes%22%3A[%7B%22priority%22%3A%22primary%22,%22slug%22%3A%22default-browser%22%7D],%22probeSets%22%3A[],%22proposedDuration%22%3A30,%22proposedEnrollment%22%3A14,%22referenceBranch%22%3A%22control%22,%22schemaVersion%22%3A%221.12.0%22,%22slug%22%3A%22on-boarding-challenge-the-default%22,%22startDate%22%3A%222023-06-21%22,%22targeting%22%3A%22true%22,%22userFacingDescription%22%3A%22Testing%20copy%20and%20images%20in%20the%20first%20run%20onboarding%20that%20is%20consistent%20with%20marketing%20messaging.%22,%22userFacingName%22%3A%22On-boarding%20Challenge%20the%20Default%22%7D]%7D&--reset-db&--log-state"
        val observed = createCommandLineArgs(Uri.parse(url))
        assertNotNull(observed)
        assertNotNull(observed?.experiments)
    }
}
