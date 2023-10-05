/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.fxaclient

import io.mockk.clearMocks
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.coVerifySequence
import io.mockk.confirmVerified
import io.mockk.every
import io.mockk.just
import io.mockk.mockk
import io.mockk.runs
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.runTest
import mozilla.appservices.sync15.DeviceType
import org.junit.Assert.assertEquals
import org.junit.Test
import kotlin.time.Duration.Companion.seconds

val testLocalDevice = LocalDevice(
    id = "device-id",
    displayName = "My Phone",
    deviceType = DeviceType.MOBILE,
    capabilities = listOf(DeviceCapability.SEND_TAB),
    pushSubscription = DevicePushSubscription("endpoint", "public-key", "auth-key"),
    pushEndpointExpired = false,
)

val testException = FxaException.Other("Test error")
val networkException = FxaException.Network("Test network error")
val authException = FxaException.Authentication("Test auth error")
val oauthStateException = FxaException.Other("Test oauth state error")

val beginOAuthFlowAction = FxaAction.BeginOAuthFlow(arrayOf("scope1"), "test-entrypoint")
val beginPairingFlowAction = FxaAction.BeginPairingFlow("http:://example.com/pairing", arrayOf("scope1"), "test-entrypoint")
val completeAuthFlowAction = FxaAction.CompleteOAuthFlow(code = "test-code", state = "test-state")
val completeAuthFlowInvalidAction = FxaAction.CompleteOAuthFlow(code = "test-code", state = "bad-state")
val initializeDeviceAction = FxaAction.InitializeDevice("My Phone", DeviceType.MOBILE, listOf(DeviceCapability.SEND_TAB))
val ensureCapabilitiesAction = FxaAction.EnsureCapabilities(listOf(DeviceCapability.SEND_TAB))
val setDeviceNameAction = FxaAction.SetDeviceName("My Phone")
val setDevicePushSubscriptionAction = FxaAction.SetDevicePushSubscription("endpoint", "public-key", "auth-key")
val sendSingleTabAction = FxaAction.SendSingleTab("my-other-device", "My page", "http://example.com/sent-tab")

fun mockFirefoxAccount() = mockk<FirefoxAccount>(relaxed = true).apply {
    every { getAuthState() } returns FxaRustAuthState.DISCONNECTED
    every { beginOauthFlow(any(), any(), any()) } returns "http://example.com/oauth-flow-start"
    every { beginPairingFlow(any(), any(), any(), any()) } returns "http://example.com/pairing-flow-start"
    every { initializeDevice(any(), any(), any()) } returns testLocalDevice
    every { ensureCapabilities(any()) } returns testLocalDevice
    every { setDeviceName(any()) } returns testLocalDevice
    every { setPushSubscription(any()) } returns testLocalDevice
    every { checkAuthorizationStatus() } returns AuthorizationInfo(active = true)
}

fun mockThrowingFirefoxAccount() = mockk<FirefoxAccount>(relaxed = true).apply {
    every { beginOauthFlow(any(), any(), any()) } throws testException
    every { beginPairingFlow(any(), any(), any(), any()) } throws testException
    every { completeOauthFlow(any(), any()) } throws testException
    every { disconnect() } throws testException
    every { initializeDevice(any(), any(), any()) } throws testException
    every { ensureCapabilities(any()) } throws testException
    every { setDeviceName(any()) } throws testException
    every { setPushSubscription(any()) } throws testException
    every { checkAuthorizationStatus() } throws testException
    every { sendSingleTab(any(), any(), any()) } throws testException
}

fun mockEventHandler() = mockk<FxaEventHandler>(relaxed = true)

fun mockPersistState() = mockk<() -> Unit>(relaxed = true, name = "persistState")

typealias VerifyFunc = suspend (FxaAuthState, FirefoxAccount, FxaEventHandler, () -> Unit) -> FxaAuthState

internal data class Mocks(
    val firefoxAccount: FirefoxAccount,
    val eventHandler: FxaEventHandler,
    val persistState: () -> Unit,
    val actionProcessor: FxaActionProcessor,
) {
    // Verify the effects of processAction
    //
    // verifyBlock should verify all mock interactions, then return the expect new state of the actionProcessor.
    suspend fun verifyAction(action: FxaAction, verifyFunc: VerifyFunc) {
        val initialState = actionProcessor.currentState
        actionProcessor.processAction(action)
        val expectedState = verifyFunc(initialState, firefoxAccount, eventHandler, persistState)
        confirmVerified(firefoxAccount, eventHandler, persistState)
        assertEquals(actionProcessor.currentState, expectedState)
        clearMocks(firefoxAccount, eventHandler, persistState, answers = false, recordedCalls = true, verificationMarks = true)
    }

    companion object {
        fun create(initialState: FxaAuthState, throwing: Boolean = false): Mocks {
            val firefoxAccount = if (throwing) {
                mockThrowingFirefoxAccount()
            } else {
                mockFirefoxAccount()
            }
            val eventHandler = mockk<FxaEventHandler>(relaxed = true)
            val persistState = mockk<() -> Unit>(relaxed = true, name = "tryPersistState")
            val actionProcessor = FxaActionProcessor(firefoxAccount, eventHandler, persistState, initialState)
            return Mocks(firefoxAccount, eventHandler, persistState, actionProcessor)
        }

        // Verify the effects processAction() for all combinations of:
        //   - Each possible state
        //   - Rust returns Ok() / Err()
        @Suppress("LongParameterList")
        suspend fun verifyAll(
            action: FxaAction,
            statesWhereTheActionShouldRun: List<FxaAuthState>,
            verify: VerifyFunc,
            verifyWhenThrows: VerifyFunc,
        ) {
            for (state in FxaAuthState.values()) {
                println("verifying for $state")
                if (statesWhereTheActionShouldRun.contains(state)) {
                    create(state, false).verifyAction(action, verify)
                    if (verifyWhenThrows != NeverThrows) {
                        create(state, true).verifyAction(action, verifyWhenThrows)
                    }
                } else {
                    // If the action shouldn't run don't coVerify any actions and check that the final state
                    // is the same as the initial state
                    create(state, false).verifyAction(action, { _, _, _, _ -> state })
                }
            }
        }
    }
}

// Special case for verifyAll to indicate that an action will never throw
val NeverThrows: VerifyFunc = { _, _, _, _ -> FxaAuthState.CONNECTED }

/**
 * This is the main unit test for the FxaActionProcessor.  The goal here is to take every action and
 * verify what happens it's processed when:
 *   * The client is connected / disconnected
 *   * The Rust client throws or doesn't throw an error
 *
 * "verify" means:
 *   * the correct calls were made to the Rust client
 *   * the correct events were emitted
 *   * the state was persisted (or not)
 *   * all of the above happened in the expected order
 *   * the new state of the FxaActionProcessor is correct
 *
 * This leads to 4 tests per action, which is a lot, but [Mocks.verifyAction] keeps things
 * relatively simple.  We could test more combinations, like what happens when different errors are
 * thrown or test different field values for [FxaAuthState.Connected] and
 * [FxaAuthState.Disconnected].  However, these distinctions usually don't matter, and we don't
 * genally test all the combinations (although some individual tests do tests some of the
 * combinations).
 *
 */
class FxaActionProcessorTest {
    @Test
    fun `FxaActionProcessor handles BeginOAuthFlow`() = runTest {
        Mocks.verifyAll(
            beginOAuthFlowAction,
            listOf(FxaAuthState.DISCONNECTED, FxaAuthState.AUTHENTICATING),
            { _, inner, eventHandler, persistState ->
                coVerifySequence {
                    inner.beginOauthFlow(listOf("scope1"), "test-entrypoint", any())
                    persistState()
                    eventHandler.onFxaEvent(
                        FxaEvent.AuthEvent(
                            FxaAuthEventKind.OAUTH_STARTED,
                            FxaAuthState.AUTHENTICATING,
                        ),
                    )
                    eventHandler.onFxaEvent(
                        FxaEvent.BeginOAuthFlow(
                            "http://example.com/oauth-flow-start",
                        ),
                    )
                }
                FxaAuthState.AUTHENTICATING
            },
            { initialState, inner, eventHandler, persistState ->
                coVerifySequence {
                    inner.beginOauthFlow(listOf("scope1"), "test-entrypoint", any())
                    persistState()
                    eventHandler.onFxaEvent(
                        FxaEvent.AuthEvent(
                            FxaAuthEventKind.OAUTH_FAILED_TO_BEGIN,
                            initialState,
                        ),
                    )
                }
                initialState
            },
        )
    }

    @Test
    fun `FxaActionProcessor handles BeginPairingFlow`() = runTest {
        Mocks.verifyAll(
            beginPairingFlowAction,
            listOf(FxaAuthState.DISCONNECTED, FxaAuthState.AUTHENTICATING),
            { _, inner, eventHandler, persistState ->
                coVerifySequence {
                    inner.beginPairingFlow("http:://example.com/pairing", listOf("scope1"), "test-entrypoint", any())
                    persistState()
                    eventHandler.onFxaEvent(
                        FxaEvent.AuthEvent(
                            FxaAuthEventKind.OAUTH_STARTED,
                            FxaAuthState.AUTHENTICATING,
                        ),
                    )
                    eventHandler.onFxaEvent(
                        FxaEvent.BeginOAuthFlow(
                            "http://example.com/pairing-flow-start",
                        ),
                    )
                }
                FxaAuthState.AUTHENTICATING
            },
            { initialState, inner, eventHandler, persistState ->
                coVerifySequence {
                    inner.beginPairingFlow("http:://example.com/pairing", listOf("scope1"), "test-entrypoint", any())
                    persistState()
                    eventHandler.onFxaEvent(
                        FxaEvent.AuthEvent(
                            FxaAuthEventKind.OAUTH_FAILED_TO_BEGIN,
                            initialState,
                        ),
                    )
                }
                initialState
            },
        )
    }

    @Test
    fun `FxaActionProcessor handles CompleteOauthFlow`() = runTest {
        Mocks.verifyAll(
            completeAuthFlowAction,
            listOf(FxaAuthState.DISCONNECTED, FxaAuthState.AUTHENTICATING),
            { _, inner, eventHandler, persistState ->
                coVerifySequence {
                    inner.completeOauthFlow("test-code", "test-state")
                    persistState()
                    eventHandler.onFxaEvent(
                        FxaEvent.AuthEvent(
                            FxaAuthEventKind.OAUTH_COMPLETE,
                            FxaAuthState.CONNECTED,
                        ),
                    )
                }
                FxaAuthState.CONNECTED
            },
            { initialState, inner, eventHandler, persistState ->
                coVerifySequence {
                    inner.completeOauthFlow("test-code", "test-state")
                    persistState()
                    // `connecting` should still be true in case another oauth flow is also in
                    // progress.  In order to unset it, the application needs to send
                    // CancelOAuthFlow
                    eventHandler.onFxaEvent(
                        FxaEvent.AuthEvent(
                            FxaAuthEventKind.OAUTH_FAILED_TO_COMPLETE,
                            initialState,
                        ),
                    )
                }
                initialState
            },
        )
    }

    @Test
    fun `FxaActionProcessor handles a failed oauth complete, then a successful one`() = runTest {
        val mocks = Mocks.create(FxaAuthState.AUTHENTICATING)
        every { mocks.firefoxAccount.completeOauthFlow(any(), any()) } throws oauthStateException
        mocks.verifyAction(completeAuthFlowInvalidAction) { _, inner, eventHandler, persistState ->
            coVerifySequence {
                inner.completeOauthFlow("test-code", "bad-state")
                persistState()
                eventHandler.onFxaEvent(
                    FxaEvent.AuthEvent(
                        FxaAuthEventKind.OAUTH_FAILED_TO_COMPLETE,
                        FxaAuthState.AUTHENTICATING,
                    ),
                )
            }
            FxaAuthState.AUTHENTICATING
        }

        every { mocks.firefoxAccount.completeOauthFlow(any(), any()) } just runs
        mocks.verifyAction(completeAuthFlowAction) { _, inner, eventHandler, persistState ->
            coVerifySequence {
                inner.completeOauthFlow("test-code", "test-state")
                persistState()
                eventHandler.onFxaEvent(
                    FxaEvent.AuthEvent(
                        FxaAuthEventKind.OAUTH_COMPLETE,
                        FxaAuthState.CONNECTED,
                    ),
                )
            }
            FxaAuthState.CONNECTED
        }
    }

    @Test
    fun `FxaActionProcessor handles CancelOauthFlow`() = runTest {
        Mocks.verifyAll(
            FxaAction.CancelOAuthFlow,
            listOf(FxaAuthState.DISCONNECTED, FxaAuthState.AUTHENTICATING),
            { _, _, eventHandler, _ ->
                coVerifySequence {
                    eventHandler.onFxaEvent(
                        FxaEvent.AuthEvent(
                            FxaAuthEventKind.OAUTH_CANCELLED,
                            FxaAuthState.DISCONNECTED,
                        ),
                    )
                }
                FxaAuthState.DISCONNECTED
            },
            NeverThrows,
        )
    }

    @Test
    fun `FxaActionProcessor handles Disconnect`() = runTest {
        Mocks.verifyAll(
            FxaAction.Disconnect,
            listOf(FxaAuthState.AUTHENTICATING, FxaAuthState.CONNECTED, FxaAuthState.CHECKING_AUTH, FxaAuthState.AUTH_ISSUES),
            { _, inner, eventHandler, persistState ->
                coVerifySequence {
                    inner.disconnect()
                    persistState()
                    eventHandler.onFxaEvent(
                        FxaEvent.AuthEvent(
                            FxaAuthEventKind.DISCONNECTED,
                            FxaAuthState.DISCONNECTED,
                        ),
                    )
                }
                FxaAuthState.DISCONNECTED
            },
            NeverThrows,
        )
    }

    @Test
    fun `FxaActionProcessor handles LogoutFromAuthIssues`() = runTest {
        Mocks.verifyAll(
            FxaAction.LogoutFromAuthIssues,
            // Note: DISCONNECTED is not listed below.  If the client is already disconnected, then
            // it doesn't make sense to handle logoutFromAuthIssues() and transition them to the
            // AUTH_ISSUES state.
            listOf(FxaAuthState.AUTHENTICATING, FxaAuthState.CONNECTED, FxaAuthState.CHECKING_AUTH),
            { _, inner, eventHandler, persistState ->
                coVerifySequence {
                    inner.logoutFromAuthIssues()
                    persistState()
                    eventHandler.onFxaEvent(
                        FxaEvent.AuthEvent(
                            FxaAuthEventKind.LOGOUT_FROM_AUTH_ISSUES,
                            FxaAuthState.AUTH_ISSUES,
                        ),
                    )
                }
                FxaAuthState.AUTH_ISSUES
            },
            NeverThrows,
        )
    }

    @Test
    fun `FxaActionProcessor handles CheckAuthorization`() = runTest {
        Mocks.verifyAll(
            FxaAction.CheckAuthorization,
            listOf(FxaAuthState.CONNECTED),
            { _, inner, eventHandler, persistState ->
                coVerifySequence {
                    eventHandler.onFxaEvent(
                        FxaEvent.AuthEvent(
                            FxaAuthEventKind.AUTH_CHECK_STARTED,
                            FxaAuthState.CHECKING_AUTH,
                        ),
                    )
                    inner.checkAuthorizationStatus()
                    persistState()
                    eventHandler.onFxaEvent(
                        FxaEvent.AuthEvent(
                            FxaAuthEventKind.AUTH_CHECK_SUCCESS,
                            FxaAuthState.CONNECTED,
                        ),
                    )
                }
                FxaAuthState.CONNECTED
            },
            // If the auth check throws FxaException.Other, then then we currently consider that a
            // success, rather than kicking the user out of the account.  Other failure models are
            // tested in the test cases below
            { _, inner, eventHandler, persistState ->
                coVerifySequence {
                    eventHandler.onFxaEvent(
                        FxaEvent.AuthEvent(
                            FxaAuthEventKind.AUTH_CHECK_STARTED,
                            FxaAuthState.CHECKING_AUTH,
                        ),
                    )
                    inner.checkAuthorizationStatus()
                    persistState()
                    eventHandler.onFxaEvent(
                        FxaEvent.AuthEvent(
                            FxaAuthEventKind.AUTH_CHECK_SUCCESS,
                            FxaAuthState.CONNECTED,
                        ),
                    )
                }
                FxaAuthState.CONNECTED
            },
        )
    }

    @Test
    fun `FxaActionProcessor disconnects if checkAuthorizationStatus returns active=false`() = runTest {
        val mocks = Mocks.create(FxaAuthState.CONNECTED)
        every { mocks.firefoxAccount.checkAuthorizationStatus() } returns AuthorizationInfo(active = false)

        mocks.verifyAction(FxaAction.CheckAuthorization) {
                _, inner, eventHandler, persistState ->
            coVerifySequence {
                eventHandler.onFxaEvent(
                    FxaEvent.AuthEvent(
                        FxaAuthEventKind.AUTH_CHECK_STARTED,
                        FxaAuthState.CHECKING_AUTH,
                    ),
                )
                inner.checkAuthorizationStatus()
                inner.logoutFromAuthIssues()
                persistState()
                eventHandler.onFxaEvent(
                    FxaEvent.AuthEvent(
                        FxaAuthEventKind.AUTH_CHECK_FAILED,
                        FxaAuthState.AUTH_ISSUES,
                    ),
                )
            }
            FxaAuthState.AUTH_ISSUES
        }
    }

    @Test
    fun `FxaActionProcessor disconnects if checkAuthorizationStatus throwns an auth exception`() = runTest {
        val mocks = Mocks.create(FxaAuthState.CONNECTED)
        every { mocks.firefoxAccount.checkAuthorizationStatus() } throws authException
        mocks.verifyAction(FxaAction.CheckAuthorization) {
                _, inner, eventHandler, persistState ->
            coVerifySequence {
                eventHandler.onFxaEvent(
                    FxaEvent.AuthEvent(
                        FxaAuthEventKind.AUTH_CHECK_STARTED,
                        FxaAuthState.CHECKING_AUTH,
                    ),
                )
                inner.checkAuthorizationStatus()
                inner.logoutFromAuthIssues()
                persistState()
                eventHandler.onFxaEvent(
                    FxaEvent.AuthEvent(
                        FxaAuthEventKind.AUTH_CHECK_FAILED,
                        FxaAuthState.AUTH_ISSUES,
                    ),
                )
            }
            FxaAuthState.AUTH_ISSUES
        }
    }

    @Test
    fun `FxaActionProcessor handles InitializeDevice`() = runTest {
        Mocks.verifyAll(
            initializeDeviceAction,
            listOf(FxaAuthState.CONNECTED),
            { _, inner, eventHandler, _ ->
                coVerifySequence {
                    inner.initializeDevice("My Phone", DeviceType.MOBILE, listOf(DeviceCapability.SEND_TAB))
                    eventHandler.onFxaEvent(FxaEvent.DeviceOperationComplete(FxaDeviceOperation.INITIALIZE_DEVICE, testLocalDevice))
                }
                FxaAuthState.CONNECTED
            },
            { _, inner, eventHandler, _ ->
                coVerifySequence {
                    inner.initializeDevice("My Phone", DeviceType.MOBILE, listOf(DeviceCapability.SEND_TAB))
                    eventHandler.onFxaEvent(FxaEvent.DeviceOperationFailed(FxaDeviceOperation.INITIALIZE_DEVICE))
                }
                FxaAuthState.CONNECTED
            },
        )
    }

    @Test
    fun `FxaActionProcessor handles EnsureCapabilities`() = runTest {
        Mocks.verifyAll(
            ensureCapabilitiesAction,
            listOf(FxaAuthState.CONNECTED),
            { _, inner, eventHandler, _ ->
                coVerifySequence {
                    inner.ensureCapabilities(listOf(DeviceCapability.SEND_TAB))
                    eventHandler.onFxaEvent(
                        FxaEvent.DeviceOperationComplete(FxaDeviceOperation.ENSURE_CAPABILITIES, testLocalDevice),
                    )
                }
                FxaAuthState.CONNECTED
            },
            { _, inner, eventHandler, _ ->
                coVerifySequence {
                    inner.ensureCapabilities(listOf(DeviceCapability.SEND_TAB))
                    eventHandler.onFxaEvent(
                        FxaEvent.DeviceOperationFailed(FxaDeviceOperation.ENSURE_CAPABILITIES),
                    )
                }
                FxaAuthState.CONNECTED
            },
        )
    }

    @Test
    fun `FxaActionProcessor handles SetDeviceName`() = runTest {
        Mocks.verifyAll(
            setDeviceNameAction,
            listOf(FxaAuthState.CONNECTED),
            { _, inner, eventHandler, _ ->
                coVerifySequence {
                    inner.setDeviceName("My Phone")
                    eventHandler.onFxaEvent(
                        FxaEvent.DeviceOperationComplete(FxaDeviceOperation.SET_DEVICE_NAME, testLocalDevice),
                    )
                }
                FxaAuthState.CONNECTED
            },
            { _, inner, eventHandler, _ ->
                coVerifySequence {
                    inner.setDeviceName("My Phone")
                    eventHandler.onFxaEvent(
                        FxaEvent.DeviceOperationFailed(FxaDeviceOperation.SET_DEVICE_NAME),
                    )
                }
                FxaAuthState.CONNECTED
            },
        )
    }

    @Test
    fun `FxaActionProcessor handles SetDevicePushSubscription`() = runTest {
        Mocks.verifyAll(
            setDevicePushSubscriptionAction,
            listOf(FxaAuthState.CONNECTED),
            { _, inner, eventHandler, _ ->
                coVerifySequence {
                    inner.setPushSubscription(
                        DevicePushSubscription("endpoint", "public-key", "auth-key"),
                    )
                    eventHandler.onFxaEvent(FxaEvent.DeviceOperationComplete(FxaDeviceOperation.SET_DEVICE_PUSH_SUBSCRIPTION, testLocalDevice))
                }
                FxaAuthState.CONNECTED
            },
            { _, inner, eventHandler, _ ->
                coVerifySequence {
                    inner.setPushSubscription(
                        DevicePushSubscription("endpoint", "public-key", "auth-key"),
                    )
                    eventHandler.onFxaEvent(FxaEvent.DeviceOperationFailed(FxaDeviceOperation.SET_DEVICE_PUSH_SUBSCRIPTION))
                }
                FxaAuthState.CONNECTED
            },
        )
    }

    @Test
    fun `FxaActionProcessor handles SendSingleTab`() = runTest {
        Mocks.verifyAll(
            sendSingleTabAction,
            listOf(FxaAuthState.CONNECTED),
            { _, inner, _, _ ->
                coVerifySequence {
                    inner.sendSingleTab("my-other-device", "My page", "http://example.com/sent-tab")
                }
                FxaAuthState.CONNECTED
            },
            { _, inner, _, _ ->
                coVerifySequence {
                    inner.sendSingleTab("my-other-device", "My page", "http://example.com/sent-tab")
                    // Should we notify clients if this fails?  There doesn't seem like there's much
                    // they can do about it.
                }
                FxaAuthState.CONNECTED
            },
        )
    }

    @Test
    fun `FxaActionProcessor sends OAuth results to the deferred`() = runTest {
        val firefoxAccount = mockFirefoxAccount()
        val actionProcessor = FxaActionProcessor(
            firefoxAccount,
            mockEventHandler(),
            mockPersistState(),
            FxaAuthState.DISCONNECTED,
        )

        CompletableDeferred<String?>().let {
            actionProcessor.processAction(beginOAuthFlowAction.copy(result = it))
            assertEquals(it.await(), "http://example.com/oauth-flow-start")
        }

        CompletableDeferred<String?>().let {
            actionProcessor.processAction(beginPairingFlowAction.copy(result = it))
            assertEquals(it.await(), "http://example.com/pairing-flow-start")
        }

        every { firefoxAccount.beginOauthFlow(any(), any(), any()) } throws testException
        every { firefoxAccount.beginPairingFlow(any(), any(), any(), any()) } throws testException

        CompletableDeferred<String?>().let {
            actionProcessor.processAction(beginOAuthFlowAction.copy(result = it))
            assertEquals(it.await(), null)
        }

        CompletableDeferred<String?>().let {
            actionProcessor.processAction(beginPairingFlowAction.copy(result = it))
            assertEquals(it.await(), null)
        }
    }

    @Test
    fun `FxaActionProcessor sends Device operation results to the deferred`() = runTest {
        val firefoxAccount = mockFirefoxAccount()
        val actionProcessor = FxaActionProcessor(
            firefoxAccount,
            mockEventHandler(),
            mockPersistState(),
            FxaAuthState.CONNECTED,
        )

        CompletableDeferred<Boolean>().let {
            actionProcessor.processAction(initializeDeviceAction.copy(result = it))
            assertEquals(it.await(), true)
        }

        CompletableDeferred<Boolean>().let {
            actionProcessor.processAction(ensureCapabilitiesAction.copy(result = it))
            assertEquals(it.await(), true)
        }

        CompletableDeferred<Boolean>().let {
            actionProcessor.processAction(setDeviceNameAction.copy(result = it))
            assertEquals(it.await(), true)
        }

        CompletableDeferred<Boolean>().let {
            actionProcessor.processAction(setDevicePushSubscriptionAction.copy(result = it))
            assertEquals(it.await(), true)
        }

        CompletableDeferred<Boolean>().let {
            actionProcessor.processAction(sendSingleTabAction.copy(result = it))
            assertEquals(it.await(), true)
        }

        firefoxAccount.apply {
            every { initializeDevice(any(), any(), any()) } throws testException
            every { ensureCapabilities(any()) } throws testException
            every { setDeviceName(any()) } throws testException
            every { setPushSubscription(any()) } throws testException
            every { sendSingleTab(any(), any(), any()) } throws testException
        }

        CompletableDeferred<Boolean>().let {
            actionProcessor.processAction(initializeDeviceAction.copy(result = it))
            assertEquals(it.await(), false)
        }

        CompletableDeferred<Boolean>().let {
            actionProcessor.processAction(ensureCapabilitiesAction.copy(result = it))
            assertEquals(it.await(), false)
        }

        CompletableDeferred<Boolean>().let {
            actionProcessor.processAction(setDeviceNameAction.copy(result = it))
            assertEquals(it.await(), false)
        }

        CompletableDeferred<Boolean>().let {
            actionProcessor.processAction(setDevicePushSubscriptionAction.copy(result = it))
            assertEquals(it.await(), false)
        }

        CompletableDeferred<Boolean>().let {
            actionProcessor.processAction(sendSingleTabAction.copy(result = it))
            assertEquals(it.await(), false)
        }
    }

    @Test
    fun `FxaActionProcessor catches errors when sending events`() = runTest {
        val eventHandler = mockk<FxaEventHandler>().apply {
            coEvery { onFxaEvent(any()) } answers {
                throw testException
            }
        }
        FxaActionProcessor(mockk(), eventHandler, mockk(), initialState = FxaAuthState.CONNECTED).sendEvent(
            FxaEvent.AuthEvent(
                FxaAuthEventKind.AUTH_CHECK_FAILED,
                FxaAuthState.AUTH_ISSUES,
            ),
        )
        // Check that the handler was called and threw an exception, but sendEvent caught it
        coVerify {
            eventHandler.onFxaEvent(any())
        }
    }

    @Test
    fun `FxaActionProcessor has a manager job that restarts when processChannel throws`() = runTest {
        var firstTime = true
        val actionProcessor = mockk<FxaActionProcessor>(relaxed = true)
        coEvery { actionProcessor.processChannel() } answers {
            if (firstTime) {
                // First time around we throw
                firstTime = false
                @Suppress("TooGenericExceptionThrown")
                throw Exception("Test errror")
            } else {
                // Second time around we quit gracefully
            }
        }
        // Run the manager job, it should restart `processChannel` when it throws the first time,
        // and return when it successfully returns
        val testDispatcher = StandardTestDispatcher()
        runActionProcessorManager(actionProcessor, testDispatcher)
        testDispatcher.scheduler.advanceUntilIdle()
        coVerify(exactly = 2) { actionProcessor.processChannel() }
    }
}

class FxaRetryTest {
    @Test
    fun `FxaActionProcessor retries 3 times after network errors`() = runTest {
        val mocks = Mocks.create(FxaAuthState.CONNECTED)
        every { mocks.firefoxAccount.setDeviceName(any()) } throwsMany listOf(networkException, networkException, networkException) andThen testLocalDevice

        mocks.verifyAction(setDeviceNameAction) { _, inner, eventHandler, _ ->
            coVerifySequence {
                // These throws FxaException.Network, we should retry
                inner.setDeviceName("My Phone")
                inner.setDeviceName("My Phone")
                inner.setDeviceName("My Phone")
                // This time it work
                inner.setDeviceName("My Phone")
                eventHandler.onFxaEvent(
                    FxaEvent.DeviceOperationComplete(FxaDeviceOperation.SET_DEVICE_NAME, testLocalDevice),
                )
            }
            FxaAuthState.CONNECTED
        }

        // Each action gets a fresh retry count.  Test out another action that fails 3 times then
        // succeeds.
        every { mocks.firefoxAccount.setDeviceName(any()) } throwsMany listOf(networkException, networkException, networkException) andThen testLocalDevice

        mocks.verifyAction(setDeviceNameAction) { _, inner, eventHandler, _ ->
            coVerifySequence {
                // These throws FxaException.Network, we should retry
                inner.setDeviceName("My Phone")
                inner.setDeviceName("My Phone")
                inner.setDeviceName("My Phone")
                // This time it work
                inner.setDeviceName("My Phone")
                eventHandler.onFxaEvent(
                    FxaEvent.DeviceOperationComplete(FxaDeviceOperation.SET_DEVICE_NAME, testLocalDevice),
                )
            }
            FxaAuthState.CONNECTED
        }
    }

    @Test
    fun `FxaActionProcessor fails after 4 network errors in a row`() = runTest {
        val mocks = Mocks.create(FxaAuthState.CONNECTED)
        every { mocks.firefoxAccount.setDeviceName(any()) } throws networkException

        mocks.verifyAction(setDeviceNameAction) { _, inner, eventHandler, _ ->
            coVerifySequence {
                // These throws FxaException.Network and we should retry
                inner.setDeviceName("My Phone")
                inner.setDeviceName("My Phone")
                inner.setDeviceName("My Phone")
                // On the 4th error, we give up
                inner.setDeviceName("My Phone")
                eventHandler.onFxaEvent(FxaEvent.DeviceOperationFailed(FxaDeviceOperation.SET_DEVICE_NAME))
            }
            FxaAuthState.CONNECTED
        }
    }

    @Test
    fun `FxaActionProcessor calls checkAuthorizationStatus after auth errors`() = runTest {
        val mocks = Mocks.create(FxaAuthState.CONNECTED)
        every { mocks.firefoxAccount.setDeviceName(any()) } throws authException andThen testLocalDevice

        mocks.verifyAction(setDeviceNameAction) { _, inner, eventHandler, persistState ->
            coVerifySequence {
                // This throws FxaException.Authentication, we should recheck the auth status
                inner.setDeviceName("My Phone")
                // The auth check works
                eventHandler.onFxaEvent(
                    FxaEvent.AuthEvent(
                        FxaAuthEventKind.AUTH_CHECK_STARTED,
                        FxaAuthState.CHECKING_AUTH,
                    ),
                )
                inner.checkAuthorizationStatus()
                persistState()
                eventHandler.onFxaEvent(
                    FxaEvent.AuthEvent(
                        FxaAuthEventKind.AUTH_CHECK_SUCCESS,
                        FxaAuthState.CONNECTED,
                    ),
                )
                // .. continue on
                inner.setDeviceName("My Phone")
                eventHandler.onFxaEvent(FxaEvent.DeviceOperationComplete(FxaDeviceOperation.SET_DEVICE_NAME, testLocalDevice))
            }
            FxaAuthState.CONNECTED
        }
    }

    @Test
    fun `FxaActionProcessor fails after 2 auth errors`() = runTest {
        val mocks = Mocks.create(FxaAuthState.CONNECTED)
        every { mocks.firefoxAccount.setDeviceName(any()) } throws authException

        mocks.verifyAction(setDeviceNameAction) { _, inner, eventHandler, persistState ->
            coVerifySequence {
                // This throws FxaException.Authentication, we should recheck the auth status
                inner.setDeviceName("My Phone")
                // The auth check works
                eventHandler.onFxaEvent(
                    FxaEvent.AuthEvent(
                        FxaAuthEventKind.AUTH_CHECK_STARTED,
                        FxaAuthState.CHECKING_AUTH,
                    ),
                )
                inner.checkAuthorizationStatus()
                persistState()
                eventHandler.onFxaEvent(
                    FxaEvent.AuthEvent(
                        FxaAuthEventKind.AUTH_CHECK_SUCCESS,
                        FxaAuthState.CONNECTED,
                    ),
                )
                // .. but this throws again,
                inner.setDeviceName("My Phone")
                // ..so save the state, transition to disconnected, and make the operation fail
                inner.logoutFromAuthIssues()
                persistState()
                eventHandler.onFxaEvent(
                    FxaEvent.AuthEvent(
                        FxaAuthEventKind.AUTH_CHECK_FAILED,
                        FxaAuthState.AUTH_ISSUES,
                    ),
                )
                eventHandler.onFxaEvent(FxaEvent.DeviceOperationFailed(FxaDeviceOperation.SET_DEVICE_NAME))
            }
            FxaAuthState.AUTH_ISSUES
        }
    }

    @Test
    fun `FxaActionProcessor fails if the auth check fails`() = runTest {
        val mocks = Mocks.create(FxaAuthState.CONNECTED)
        every { mocks.firefoxAccount.setDeviceName(any()) } throws authException andThen testLocalDevice
        every { mocks.firefoxAccount.checkAuthorizationStatus() } returns AuthorizationInfo(active = false)

        mocks.verifyAction(setDeviceNameAction) { _, inner, eventHandler, persistState ->
            coVerify {
                // This throws FxaException.Authentication, we should recheck the auth status
                inner.setDeviceName("My Phone")
                // The auth check fails
                eventHandler.onFxaEvent(
                    FxaEvent.AuthEvent(
                        FxaAuthEventKind.AUTH_CHECK_STARTED,
                        FxaAuthState.CHECKING_AUTH,
                    ),
                )
                inner.checkAuthorizationStatus()
                inner.logoutFromAuthIssues()
                persistState()
                eventHandler.onFxaEvent(
                    FxaEvent.AuthEvent(
                        FxaAuthEventKind.AUTH_CHECK_FAILED,
                        FxaAuthState.AUTH_ISSUES,
                    ),
                )
                // .. so the operation fails
                eventHandler.onFxaEvent(FxaEvent.DeviceOperationFailed(FxaDeviceOperation.SET_DEVICE_NAME))
            }
            FxaAuthState.AUTH_ISSUES
        }
    }

    @Test
    fun `FxaActionProcessor fails after multiple auth errors in a short time`() = runTest {
        val mocks = Mocks.create(FxaAuthState.CONNECTED)
        every { mocks.firefoxAccount.setDeviceName(any()) } throws authException andThen testLocalDevice

        mocks.verifyAction(setDeviceNameAction) { _, inner, eventHandler, persistState ->
            coVerify {
                // This throws FxaException.Authentication, we should recheck the auth status
                inner.setDeviceName("My Phone")
                // The auth check works
                eventHandler.onFxaEvent(
                    FxaEvent.AuthEvent(
                        FxaAuthEventKind.AUTH_CHECK_STARTED,
                        FxaAuthState.CHECKING_AUTH,
                    ),
                )
                inner.checkAuthorizationStatus()
                persistState()
                eventHandler.onFxaEvent(
                    FxaEvent.AuthEvent(
                        FxaAuthEventKind.AUTH_CHECK_SUCCESS,
                        FxaAuthState.CONNECTED,
                    ),
                )
                // .. continue on
                inner.setDeviceName("My Phone")
                eventHandler.onFxaEvent(FxaEvent.DeviceOperationComplete(FxaDeviceOperation.SET_DEVICE_NAME, testLocalDevice))
            }
            FxaAuthState.CONNECTED
        }

        mocks.actionProcessor.retryLogic.fastForward(59.seconds)
        every {
            mocks.firefoxAccount.setDeviceName(any())
        } throws authException andThen testLocalDevice

        mocks.verifyAction(setDeviceNameAction) { _, inner, eventHandler, persistState ->
            coVerify {
                // This throws,
                inner.setDeviceName("My Phone")
                // ..so save the state, transition to disconnected, and make the operation fail
                inner.logoutFromAuthIssues()
                persistState()
                eventHandler.onFxaEvent(
                    FxaEvent.AuthEvent(
                        FxaAuthEventKind.AUTH_CHECK_FAILED,
                        FxaAuthState.AUTH_ISSUES,
                    ),
                )
                eventHandler.onFxaEvent(FxaEvent.DeviceOperationFailed(FxaDeviceOperation.SET_DEVICE_NAME))
            }
            FxaAuthState.AUTH_ISSUES
        }
    }

    @Test
    fun `FxaActionProcessor checks authorization again after timeout period passes`() = runTest {
        val mocks = Mocks.create(FxaAuthState.CONNECTED)
        every {
            mocks.firefoxAccount.setDeviceName(any())
        } throws authException andThen testLocalDevice

        mocks.verifyAction(setDeviceNameAction) { _, inner, eventHandler, persistState ->
            coVerify {
                // This throws FxaException.Authentication, we should recheck the auth status
                inner.setDeviceName("My Phone")
                // The auth check works
                eventHandler.onFxaEvent(
                    FxaEvent.AuthEvent(
                        FxaAuthEventKind.AUTH_CHECK_STARTED,
                        FxaAuthState.CHECKING_AUTH,
                    ),
                )
                inner.checkAuthorizationStatus()
                persistState()
                eventHandler.onFxaEvent(
                    FxaEvent.AuthEvent(
                        FxaAuthEventKind.AUTH_CHECK_SUCCESS,
                        FxaAuthState.CONNECTED,
                    ),
                )
                // .. continue on
                inner.setDeviceName("My Phone")
                eventHandler.onFxaEvent(FxaEvent.DeviceOperationComplete(FxaDeviceOperation.SET_DEVICE_NAME, testLocalDevice))
            }
            FxaAuthState.CONNECTED
        }

        mocks.actionProcessor.retryLogic.fastForward(61.seconds)
        every {
            mocks.firefoxAccount.setDeviceName(any())
        } throws authException andThen testLocalDevice

        mocks.verifyAction(setDeviceNameAction) { _, inner, eventHandler, persistState ->
            coVerify {
                // Timeout period over, we should recheck the auth status this time
                inner.setDeviceName("My Phone")
                // The auth check works
                eventHandler.onFxaEvent(
                    FxaEvent.AuthEvent(
                        FxaAuthEventKind.AUTH_CHECK_STARTED,
                        FxaAuthState.CHECKING_AUTH,
                    ),
                )
                inner.checkAuthorizationStatus()
                persistState()
                eventHandler.onFxaEvent(
                    FxaEvent.AuthEvent(
                        FxaAuthEventKind.AUTH_CHECK_SUCCESS,
                        FxaAuthState.CONNECTED,
                    ),
                )
                // .. continue on
                inner.setDeviceName("My Phone")
                eventHandler.onFxaEvent(FxaEvent.DeviceOperationComplete(FxaDeviceOperation.SET_DEVICE_NAME, testLocalDevice))
            }
            FxaAuthState.CONNECTED
        }
    }

    @Test
    fun `FxaActionProcessor retries after an auth + network exception`() = runTest {
        val mocks = Mocks.create(FxaAuthState.CONNECTED)
        every {
            mocks.firefoxAccount.setDeviceName(any())
        } throws authException andThen testLocalDevice

        every {
            mocks.firefoxAccount.checkAuthorizationStatus()
        } throws networkException andThen AuthorizationInfo(active = true)

        mocks.verifyAction(setDeviceNameAction) { _, inner, eventHandler, persistState ->
            coVerify {
                // This throws FxaException.Authentication, we should recheck the auth status
                inner.setDeviceName("My Phone")
                eventHandler.onFxaEvent(
                    FxaEvent.AuthEvent(
                        FxaAuthEventKind.AUTH_CHECK_STARTED,
                        FxaAuthState.CHECKING_AUTH,
                    ),
                )
                // This throws a network error, we should retry
                inner.checkAuthorizationStatus()
                // This time it works
                inner.checkAuthorizationStatus()
                persistState()
                eventHandler.onFxaEvent(
                    FxaEvent.AuthEvent(
                        FxaAuthEventKind.AUTH_CHECK_SUCCESS,
                        FxaAuthState.CONNECTED,
                    ),
                )
                // .. continue on
                inner.setDeviceName("My Phone")
                eventHandler.onFxaEvent(FxaEvent.DeviceOperationComplete(FxaDeviceOperation.SET_DEVICE_NAME, testLocalDevice))
            }
            FxaAuthState.CONNECTED
        }
    }

    @Test
    fun `FxaActionProcessor retries after a network + auth exception`() = runTest {
        val mocks = Mocks.create(FxaAuthState.CONNECTED)
        every {
            mocks.firefoxAccount.setDeviceName(any())
        } throws authException andThen testLocalDevice

        every {
            mocks.firefoxAccount.checkAuthorizationStatus()
        } throws networkException andThen AuthorizationInfo(active = true)

        mocks.verifyAction(setDeviceNameAction) { _, inner, eventHandler, persistState ->
            coVerify {
                // This throws FxaException.Network should try again
                inner.setDeviceName("My Phone")
                // This throws FxaException.Authentication, we should recheck the auth status
                inner.setDeviceName("My Phone")
                eventHandler.onFxaEvent(
                    FxaEvent.AuthEvent(
                        FxaAuthEventKind.AUTH_CHECK_STARTED,
                        FxaAuthState.CHECKING_AUTH,
                    ),
                )
                // This works
                inner.checkAuthorizationStatus()
                persistState()
                eventHandler.onFxaEvent(
                    FxaEvent.AuthEvent(
                        FxaAuthEventKind.AUTH_CHECK_SUCCESS,
                        FxaAuthState.CONNECTED,
                    ),
                )
                // .. continue on
                inner.setDeviceName("My Phone")
                eventHandler.onFxaEvent(FxaEvent.DeviceOperationComplete(FxaDeviceOperation.SET_DEVICE_NAME, testLocalDevice))
            }
            FxaAuthState.CONNECTED
        }
    }
}

class MetricsParamsTest {
    @Test
    fun `FxaActionProcessor handles BeginOAuthFlow metrics`() = runTest {
        val mocks = Mocks.create(FxaAuthState.DISCONNECTED)
        val testMetrics = MetricsParams(
            parameters = mapOf("foo" to "bar"),
        )
        mocks.actionProcessor.processAction(beginOAuthFlowAction.copy(metrics = testMetrics))
        coVerify { mocks.firefoxAccount.beginOauthFlow(any(), any(), testMetrics) }
    }

    @Test
    fun `FxaActionProcessor handles BeginPairingFlow metrics`() = runTest {
        val mocks = Mocks.create(FxaAuthState.DISCONNECTED)
        val testMetrics = MetricsParams(
            parameters = mapOf("foo" to "bar"),
        )
        mocks.actionProcessor.processAction(beginPairingFlowAction.copy(metrics = testMetrics))
        coVerify { mocks.firefoxAccount.beginPairingFlow(any(), any(), any(), testMetrics) }
    }
}
