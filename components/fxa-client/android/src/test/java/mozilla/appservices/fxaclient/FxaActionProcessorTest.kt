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

internal data class Mocks(
    val firefoxAccount: FirefoxAccount,
    val eventHandler: FxaEventHandler,
    val persistState: () -> Unit,
    val actionProcessor: FxaActionProcessor,
) {
    // Verify the effects of an action
    //
    // verifyBlock should verify all mock interactions, than return the expect new state of the
    // actionProcessor.  Pass in null (AKA NoEffects) to verify that there were no mock interactions
    // and the state didn't change
    suspend fun verifyAction(action: FxaAction, verifyBlock: ActionVerifier) {
        if (verifyBlock == NeverHappens) {
            return
        }
        val initialState = actionProcessor.currentState
        actionProcessor.processAction(action)
        when (verifyBlock) {
            NoEffects -> assertEquals(initialState, actionProcessor.currentState)
            else -> {
                val expectedState = verifyBlock(firefoxAccount, eventHandler, persistState)
                assertEquals(expectedState, actionProcessor.currentState)
            }
        }
        confirmVerified(firefoxAccount, eventHandler, persistState)
        clearMocks(firefoxAccount, eventHandler, persistState, answers = false, recordedCalls = true, verificationMarks = true)
    }

    companion object {
        fun create(initialState: FxaAuthState, throwing: Boolean = false): Mocks {
            val firefoxAccount = mockk<FirefoxAccount>(relaxed = true).apply {
                if (!throwing) {
                    every { getAuthState() } returns FxaRustAuthState.Disconnected(fromAuthIssues = false)
                    every { beginOauthFlow(any(), any(), any()) } returns "http://example.com/oauth-flow-start"
                    every { beginPairingFlow(any(), any(), any(), any()) } returns "http://example.com/pairing-flow-start"
                    every { initializeDevice(any(), any(), any()) } returns testLocalDevice
                    every { ensureCapabilities(any()) } returns testLocalDevice
                    every { setDeviceName(any()) } returns testLocalDevice
                    every { setPushSubscription(any()) } returns testLocalDevice
                    every { checkAuthorizationStatus() } returns AuthorizationInfo(active = true)
                } else {
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
            }
            val eventHandler = mockk<FxaEventHandler>(relaxed = true)
            val persistState = mockk<() -> Unit>(relaxed = true, name = "tryPersistState")
            val actionProcessor = FxaActionProcessor(firefoxAccount, eventHandler, persistState, initialState)
            return Mocks(firefoxAccount, eventHandler, persistState, actionProcessor)
        }

        // Check the effects processAction() calls for each possible state
        //
        // This checks each combination of:
        //   - connected / disconnected
        //   - Rust returns Ok() / Err()
        //
        // Note: this doesn't differentiate based on the FxaAuthState fields values, like
        // `fromAuthIssues`. FxaActionProcessor sets these fields, but otherwise ignores them.
        @Suppress("LongParameterList")
        internal suspend fun verifyAction(
            action: FxaAction,
            whenDisconnected: ActionVerifier,
            whenDisconnectedIfThrows: ActionVerifier,
            whenConnected: ActionVerifier,
            whenConnectedIfThrows: ActionVerifier,
            initialStateDisconnected: FxaAuthState = FxaAuthState.Disconnected(),
            initialStateConnected: FxaAuthState = FxaAuthState.Connected(),
        ) {
            create(initialStateDisconnected, false).verifyAction(action, whenDisconnected)
            create(initialStateDisconnected, true).verifyAction(action, whenDisconnectedIfThrows)
            create(initialStateConnected, false).verifyAction(action, whenConnected)
            create(initialStateConnected, true).verifyAction(action, whenConnectedIfThrows)
        }
    }
}

typealias ActionVerifier = suspend (FirefoxAccount, FxaEventHandler, () -> Unit) -> FxaAuthState

// Used when an action should have no effects at all
val NoEffects: ActionVerifier = { _, _, _ -> FxaAuthState.Disconnected() }

// Used when an action should never happen, for example FxaAction.Disconnect should never throw, so
// we use this rather than making a verifier to test an impossible pathe
val NeverHappens: ActionVerifier = { _, _, _ -> FxaAuthState.Disconnected() }

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
        Mocks.verifyAction(
            beginOAuthFlowAction,
            whenDisconnected = { inner, eventHandler, persistState ->
                coVerifySequence {
                    inner.beginOauthFlow(listOf("scope1"), "test-entrypoint", any())
                    persistState()
                    eventHandler.onFxaEvent(
                        FxaEvent.AuthStateChanged(
                            newState = FxaAuthState.Disconnected(connecting = true),
                            transition = FxaAuthStateTransition.OAUTH_STARTED,
                        ),
                    )
                    eventHandler.onFxaEvent(
                        FxaEvent.BeginOAuthFlow(
                            "http://example.com/oauth-flow-start",
                        ),
                    )
                }
                FxaAuthState.Disconnected(connecting = true)
            },
            whenDisconnectedIfThrows = { inner, eventHandler, persistState ->
                coVerifySequence {
                    inner.beginOauthFlow(listOf("scope1"), "test-entrypoint", any())
                    persistState()
                    eventHandler.onFxaEvent(
                        FxaEvent.AuthStateChanged(
                            newState = FxaAuthState.Disconnected(),
                            transition = FxaAuthStateTransition.OAUTH_FAILED_TO_BEGIN,
                        ),
                    )
                }
                FxaAuthState.Disconnected()
            },
            whenConnected = NoEffects,
            whenConnectedIfThrows = NeverHappens,
        )
    }

    @Test
    fun `FxaActionProcessor handles BeginPairingFlow`() = runTest {
        Mocks.verifyAction(
            beginPairingFlowAction,
            whenDisconnected = { inner, eventHandler, persistState ->
                coVerifySequence {
                    inner.beginPairingFlow("http:://example.com/pairing", listOf("scope1"), "test-entrypoint", any())
                    persistState()
                    eventHandler.onFxaEvent(
                        FxaEvent.AuthStateChanged(
                            newState = FxaAuthState.Disconnected(connecting = true),
                            transition = FxaAuthStateTransition.OAUTH_STARTED,
                        ),
                    )
                    eventHandler.onFxaEvent(
                        FxaEvent.BeginOAuthFlow(
                            "http://example.com/pairing-flow-start",
                        ),
                    )
                }
                FxaAuthState.Disconnected(connecting = true)
            },
            whenDisconnectedIfThrows = { inner, eventHandler, persistState ->
                coVerifySequence {
                    inner.beginPairingFlow("http:://example.com/pairing", listOf("scope1"), "test-entrypoint", any())
                    persistState()
                    eventHandler.onFxaEvent(
                        FxaEvent.AuthStateChanged(
                            newState = FxaAuthState.Disconnected(connecting = false),
                            transition = FxaAuthStateTransition.OAUTH_FAILED_TO_BEGIN,
                        ),
                    )
                }
                FxaAuthState.Disconnected(connecting = false)
            },
            whenConnected = NoEffects,
            whenConnectedIfThrows = NeverHappens,
        )
    }

    @Test
    fun `FxaActionProcessor handles CompleteOauthFlow`() = runTest {
        Mocks.verifyAction(
            completeAuthFlowAction,
            initialStateDisconnected = FxaAuthState.Disconnected(connecting = true),
            whenDisconnected = { inner, eventHandler, persistState ->
                coVerifySequence {
                    inner.completeOauthFlow("test-code", "test-state")
                    persistState()
                    eventHandler.onFxaEvent(
                        FxaEvent.AuthStateChanged(
                            newState = FxaAuthState.Connected(),
                            transition = FxaAuthStateTransition.OAUTH_COMPLETE,
                        ),
                    )
                }
                FxaAuthState.Connected()
            },
            whenDisconnectedIfThrows = { inner, eventHandler, persistState ->
                coVerifySequence {
                    inner.completeOauthFlow("test-code", "test-state")
                    persistState()
                    // `connecting` should still be true in case another oauth flow is also in
                    // progress.  In order to unset it, the application needs to send
                    // CancelOAuthFlow
                    eventHandler.onFxaEvent(
                        FxaEvent.AuthStateChanged(
                            newState = FxaAuthState.Disconnected(connecting = true),
                            transition = FxaAuthStateTransition.OAUTH_FAILED_TO_COMPLETE,
                        ),
                    )
                }
                FxaAuthState.Disconnected(connecting = true)
            },
            whenConnected = NoEffects,
            whenConnectedIfThrows = NeverHappens,
        )
    }

    @Test
    fun `FxaActionProcessor handles a failed oauth complete, then a successful one`() = runTest {
        val mocks = Mocks.create(FxaAuthState.Disconnected(connecting = true))
        every { mocks.firefoxAccount.completeOauthFlow(any(), any()) } throws oauthStateException

        mocks.verifyAction(completeAuthFlowInvalidAction) { inner, eventHandler, persistState ->
            coVerifySequence {
                inner.completeOauthFlow("test-code", "bad-state")
                persistState()
                eventHandler.onFxaEvent(
                    FxaEvent.AuthStateChanged(
                        newState = FxaAuthState.Disconnected(connecting = true),
                        transition = FxaAuthStateTransition.OAUTH_FAILED_TO_COMPLETE,
                    ),
                )
            }
            FxaAuthState.Disconnected(connecting = true)
        }

        every { mocks.firefoxAccount.completeOauthFlow(any(), any()) } just runs
        mocks.verifyAction(completeAuthFlowAction) { inner, eventHandler, persistState ->
            coVerifySequence {
                inner.completeOauthFlow("test-code", "test-state")
                persistState()
                eventHandler.onFxaEvent(
                    FxaEvent.AuthStateChanged(
                        newState = FxaAuthState.Connected(),
                        transition = FxaAuthStateTransition.OAUTH_COMPLETE,
                    ),
                )
            }
            FxaAuthState.Connected()
        }
    }

    @Test
    fun `FxaActionProcessor handles CancelOauthFlow`() = runTest {
        Mocks.verifyAction(
            FxaAction.CancelOAuthFlow,
            initialStateDisconnected = FxaAuthState.Disconnected(connecting = true),
            whenDisconnected = { _, eventHandler, _ ->
                coVerifySequence {
                    eventHandler.onFxaEvent(
                        FxaEvent.AuthStateChanged(
                            newState = FxaAuthState.Disconnected(connecting = false),
                            transition = FxaAuthStateTransition.OAUTH_CANCELLED,
                        ),
                    )
                }
                FxaAuthState.Disconnected(connecting = false)
            },
            whenDisconnectedIfThrows = NeverHappens,
            whenConnected = NoEffects,
            whenConnectedIfThrows = NeverHappens,
        )
    }

    @Test
    fun `FxaActionProcessor handles Disconnect`() = runTest {
        Mocks.verifyAction(
            FxaAction.Disconnect(),
            whenDisconnected = NoEffects,
            whenDisconnectedIfThrows = NeverHappens,
            whenConnected = { inner, eventHandler, persistState ->
                coVerifySequence {
                    inner.disconnect()
                    persistState()
                    eventHandler.onFxaEvent(
                        FxaEvent.AuthStateChanged(
                            newState = FxaAuthState.Disconnected(),
                            transition = FxaAuthStateTransition.DISCONNECTED,
                        ),
                    )
                }
                FxaAuthState.Disconnected()
            },
            whenConnectedIfThrows = NeverHappens,
        )
    }

    @Test
    fun `FxaActionProcessor handles Disconnect(fromAuthIssues=true)`() = runTest {
        Mocks.verifyAction(
            FxaAction.Disconnect(fromAuthIssues = true),
            whenDisconnected = NoEffects,
            whenDisconnectedIfThrows = NeverHappens,
            whenConnected = { inner, eventHandler, persistState ->
                coVerifySequence {
                    inner.disconnectFromAuthIssues()
                    persistState()
                    eventHandler.onFxaEvent(
                        FxaEvent.AuthStateChanged(
                            newState = FxaAuthState.Disconnected(fromAuthIssues = true),
                            transition = FxaAuthStateTransition.AUTH_CHECK_FAILED,
                        ),
                    )
                }
                FxaAuthState.Disconnected(fromAuthIssues = true)
            },
            whenConnectedIfThrows = NeverHappens,
        )
    }

    @Test
    fun `FxaActionProcessor handles CheckAuthorization`() = runTest {
        Mocks.verifyAction(
            FxaAction.CheckAuthorization,
            whenDisconnected = NoEffects,
            whenDisconnectedIfThrows = NeverHappens,
            whenConnected = { inner, eventHandler, persistState ->
                coVerifySequence {
                    eventHandler.onFxaEvent(
                        FxaEvent.AuthStateChanged(
                            newState = FxaAuthState.Connected(authCheckInProgress = true),
                            transition = FxaAuthStateTransition.AUTH_CHECK_STARTED,
                        ),
                    )
                    inner.checkAuthorizationStatus()
                    persistState()
                    eventHandler.onFxaEvent(
                        FxaEvent.AuthStateChanged(
                            newState = FxaAuthState.Connected(),
                            transition = FxaAuthStateTransition.AUTH_CHECK_SUCCESS,
                        ),
                    )
                }
                FxaAuthState.Connected()
            },
            // If the auth check throws FxaException.Other, then then we currently consider that a
            // success, rather than kicking the user out of the account.  Other failure models are
            // tested in the test cases below
            whenConnectedIfThrows = { inner, eventHandler, persistState ->
                coVerifySequence {
                    eventHandler.onFxaEvent(
                        FxaEvent.AuthStateChanged(
                            newState = FxaAuthState.Connected(authCheckInProgress = true),
                            transition = FxaAuthStateTransition.AUTH_CHECK_STARTED,
                        ),
                    )
                    inner.checkAuthorizationStatus()
                    persistState()
                    eventHandler.onFxaEvent(
                        FxaEvent.AuthStateChanged(
                            newState = FxaAuthState.Connected(),
                            transition = FxaAuthStateTransition.AUTH_CHECK_SUCCESS,
                        ),
                    )
                }
                FxaAuthState.Connected()
            },
        )
    }

    @Test
    fun `FxaActionProcessor disconnects if checkAuthorizationStatus returns active=false`() = runTest {
        val mocks = Mocks.create(FxaAuthState.Connected())
        every { mocks.firefoxAccount.checkAuthorizationStatus() } returns AuthorizationInfo(active = false)
        mocks.verifyAction(FxaAction.CheckAuthorization) { inner, eventHandler, persistState ->
            coVerifySequence {
                eventHandler.onFxaEvent(
                    FxaEvent.AuthStateChanged(
                        newState = FxaAuthState.Connected(authCheckInProgress = true),
                        transition = FxaAuthStateTransition.AUTH_CHECK_STARTED,
                    ),
                )
                inner.checkAuthorizationStatus()
                inner.disconnectFromAuthIssues()
                persistState()
                eventHandler.onFxaEvent(
                    FxaEvent.AuthStateChanged(
                        newState = FxaAuthState.Disconnected(fromAuthIssues = true),
                        transition = FxaAuthStateTransition.AUTH_CHECK_FAILED,
                    ),
                )
            }
            FxaAuthState.Disconnected(fromAuthIssues = true)
        }
    }

    @Test
    fun `FxaActionProcessor disconnects if checkAuthorizationStatus throwns an auth exception`() = runTest {
        val mocks = Mocks.create(FxaAuthState.Connected())
        every { mocks.firefoxAccount.checkAuthorizationStatus() } throws authException
        mocks.verifyAction(FxaAction.CheckAuthorization) { inner, eventHandler, persistState ->
            coVerifySequence {
                eventHandler.onFxaEvent(
                    FxaEvent.AuthStateChanged(
                        newState = FxaAuthState.Connected(authCheckInProgress = true),
                        transition = FxaAuthStateTransition.AUTH_CHECK_STARTED,
                    ),
                )
                inner.checkAuthorizationStatus()
                inner.disconnectFromAuthIssues()
                persistState()
                eventHandler.onFxaEvent(
                    FxaEvent.AuthStateChanged(
                        newState = FxaAuthState.Disconnected(fromAuthIssues = true),
                        transition = FxaAuthStateTransition.AUTH_CHECK_FAILED,
                    ),
                )
            }
            FxaAuthState.Disconnected(fromAuthIssues = true)
        }
    }

    @Test
    fun `FxaActionProcessor handles InitializeDevice`() = runTest {
        Mocks.verifyAction(
            initializeDeviceAction,
            whenDisconnected = NoEffects,
            whenDisconnectedIfThrows = NeverHappens,
            whenConnected = { inner, eventHandler, _ ->
                coVerifySequence {
                    inner.initializeDevice("My Phone", DeviceType.MOBILE, listOf(DeviceCapability.SEND_TAB))
                    eventHandler.onFxaEvent(FxaEvent.DeviceOperationComplete(FxaDeviceOperation.INITIALIZE_DEVICE, testLocalDevice))
                }
                FxaAuthState.Connected()
            },
            whenConnectedIfThrows = { inner, eventHandler, _ ->
                coVerifySequence {
                    inner.initializeDevice("My Phone", DeviceType.MOBILE, listOf(DeviceCapability.SEND_TAB))
                    eventHandler.onFxaEvent(FxaEvent.DeviceOperationFailed(FxaDeviceOperation.INITIALIZE_DEVICE))
                }
                FxaAuthState.Connected()
            },
        )
    }

    @Test
    fun `FxaActionProcessor handles EnsureCapabilities`() = runTest {
        Mocks.verifyAction(
            ensureCapabilitiesAction,
            whenDisconnected = NoEffects,
            whenDisconnectedIfThrows = NeverHappens,
            whenConnected = { inner, eventHandler, _ ->
                coVerifySequence {
                    inner.ensureCapabilities(listOf(DeviceCapability.SEND_TAB))
                    eventHandler.onFxaEvent(
                        FxaEvent.DeviceOperationComplete(FxaDeviceOperation.ENSURE_CAPABILITIES, testLocalDevice),
                    )
                }
                FxaAuthState.Connected()
            },
            whenConnectedIfThrows = { inner, eventHandler, _ ->
                coVerifySequence {
                    inner.ensureCapabilities(listOf(DeviceCapability.SEND_TAB))
                    eventHandler.onFxaEvent(
                        FxaEvent.DeviceOperationFailed(FxaDeviceOperation.ENSURE_CAPABILITIES),
                    )
                }
                FxaAuthState.Connected()
            },
        )
    }

    @Test
    fun `FxaActionProcessor handles SetDeviceName`() = runTest {
        Mocks.verifyAction(
            setDeviceNameAction,
            whenDisconnected = NoEffects,
            whenDisconnectedIfThrows = NeverHappens,
            whenConnected = { inner, eventHandler, _ ->
                coVerifySequence {
                    inner.setDeviceName("My Phone")
                    eventHandler.onFxaEvent(
                        FxaEvent.DeviceOperationComplete(FxaDeviceOperation.SET_DEVICE_NAME, testLocalDevice),
                    )
                }
                FxaAuthState.Connected()
            },
            whenConnectedIfThrows = { inner, eventHandler, _ ->
                coVerifySequence {
                    inner.setDeviceName("My Phone")
                    eventHandler.onFxaEvent(
                        FxaEvent.DeviceOperationFailed(FxaDeviceOperation.SET_DEVICE_NAME),
                    )
                }
                FxaAuthState.Connected()
            },
        )
    }

    @Test
    fun `FxaActionProcessor handles SetDevicePushSubscription`() = runTest {
        Mocks.verifyAction(
            setDevicePushSubscriptionAction,
            whenDisconnected = NoEffects,
            whenDisconnectedIfThrows = NeverHappens,
            whenConnected = { inner, eventHandler, _ ->
                coVerifySequence {
                    inner.setPushSubscription(
                        DevicePushSubscription("endpoint", "public-key", "auth-key"),
                    )
                    eventHandler.onFxaEvent(FxaEvent.DeviceOperationComplete(FxaDeviceOperation.SET_DEVICE_PUSH_SUBSCRIPTION, testLocalDevice))
                }
                FxaAuthState.Connected()
            },
            whenConnectedIfThrows = { inner, eventHandler, _ ->
                coVerifySequence {
                    inner.setPushSubscription(
                        DevicePushSubscription("endpoint", "public-key", "auth-key"),
                    )
                    eventHandler.onFxaEvent(FxaEvent.DeviceOperationFailed(FxaDeviceOperation.SET_DEVICE_PUSH_SUBSCRIPTION))
                }
                FxaAuthState.Connected()
            },
        )
    }

    @Test
    fun `FxaActionProcessor handles SendSingleTab`() = runTest {
        Mocks.verifyAction(
            sendSingleTabAction,
            whenDisconnected = NoEffects,
            whenDisconnectedIfThrows = NeverHappens,
            whenConnected = { inner, _, _ ->
                coVerifySequence {
                    inner.sendSingleTab("my-other-device", "My page", "http://example.com/sent-tab")
                }
                FxaAuthState.Connected()
            },
            whenConnectedIfThrows = { inner, _, _ ->
                coVerifySequence {
                    inner.sendSingleTab("my-other-device", "My page", "http://example.com/sent-tab")
                    // Should we notify clients if this fails?  There doesn't seem like there's much
                    // they can do about it.
                }
                FxaAuthState.Connected()
            },
        )
    }

    @Test
    fun `FxaActionProcessor sends OAuth results to the deferred`() = runTest {
        val mocks = Mocks.create(FxaAuthState.Disconnected())

        CompletableDeferred<String?>().let {
            mocks.actionProcessor.processAction(beginOAuthFlowAction.copy(result = it))
            assertEquals(it.await(), "http://example.com/oauth-flow-start")
        }

        CompletableDeferred<String?>().let {
            mocks.actionProcessor.processAction(beginPairingFlowAction.copy(result = it))
            assertEquals(it.await(), "http://example.com/pairing-flow-start")
        }

        every { mocks.firefoxAccount.beginOauthFlow(any(), any(), any()) } throws testException
        every { mocks.firefoxAccount.beginPairingFlow(any(), any(), any(), any()) } throws testException

        CompletableDeferred<String?>().let {
            mocks.actionProcessor.processAction(beginOAuthFlowAction.copy(result = it))
            assertEquals(it.await(), null)
        }

        CompletableDeferred<String?>().let {
            mocks.actionProcessor.processAction(beginPairingFlowAction.copy(result = it))
            assertEquals(it.await(), null)
        }
    }

    @Test
    fun `FxaActionProcessor sends Device operation results to the deferred`() = runTest {
        val mocks = Mocks.create(FxaAuthState.Connected())

        CompletableDeferred<Boolean>().let {
            mocks.actionProcessor.processAction(initializeDeviceAction.copy(result = it))
            assertEquals(it.await(), true)
        }

        CompletableDeferred<Boolean>().let {
            mocks.actionProcessor.processAction(ensureCapabilitiesAction.copy(result = it))
            assertEquals(it.await(), true)
        }

        CompletableDeferred<Boolean>().let {
            mocks.actionProcessor.processAction(setDeviceNameAction.copy(result = it))
            assertEquals(it.await(), true)
        }

        CompletableDeferred<Boolean>().let {
            mocks.actionProcessor.processAction(setDevicePushSubscriptionAction.copy(result = it))
            assertEquals(it.await(), true)
        }

        CompletableDeferred<Boolean>().let {
            mocks.actionProcessor.processAction(sendSingleTabAction.copy(result = it))
            assertEquals(it.await(), true)
        }

        mocks.firefoxAccount.apply {
            every { initializeDevice(any(), any(), any()) } throws testException
            every { ensureCapabilities(any()) } throws testException
            every { setDeviceName(any()) } throws testException
            every { setPushSubscription(any()) } throws testException
            every { sendSingleTab(any(), any(), any()) } throws testException
        }

        CompletableDeferred<Boolean>().let {
            mocks.actionProcessor.processAction(initializeDeviceAction.copy(result = it))
            assertEquals(it.await(), false)
        }

        CompletableDeferred<Boolean>().let {
            mocks.actionProcessor.processAction(ensureCapabilitiesAction.copy(result = it))
            assertEquals(it.await(), false)
        }

        CompletableDeferred<Boolean>().let {
            mocks.actionProcessor.processAction(setDeviceNameAction.copy(result = it))
            assertEquals(it.await(), false)
        }

        CompletableDeferred<Boolean>().let {
            mocks.actionProcessor.processAction(setDevicePushSubscriptionAction.copy(result = it))
            assertEquals(it.await(), false)
        }

        CompletableDeferred<Boolean>().let {
            mocks.actionProcessor.processAction(sendSingleTabAction.copy(result = it))
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
        FxaActionProcessor(mockk(), eventHandler, mockk(), initialState = FxaAuthState.Connected()).sendEvent(
            FxaEvent.AuthStateChanged(
                newState = FxaAuthState.Disconnected(fromAuthIssues = true),
                transition = FxaAuthStateTransition.AUTH_CHECK_FAILED,
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
    fun `FxaActionProcessor retries after network errors`() = runTest {
        val mocks = Mocks.create(FxaAuthState.Connected())
        every {
            mocks.firefoxAccount.setDeviceName(any())
        } throws networkException andThen testLocalDevice

        mocks.verifyAction(setDeviceNameAction) { inner, eventHandler, _ ->
            coVerifySequence {
                // This throws FxaException.Network, we should retry
                inner.setDeviceName("My Phone")
                // This time it work
                inner.setDeviceName("My Phone")
                eventHandler.onFxaEvent(
                    FxaEvent.DeviceOperationComplete(FxaDeviceOperation.SET_DEVICE_NAME, testLocalDevice),
                )
            }
            FxaAuthState.Connected()
        }
    }

    @Test
    fun `FxaActionProcessor fails after 2 network errors`() = runTest {
        val mocks = Mocks.create(FxaAuthState.Connected())
        every { mocks.firefoxAccount.setDeviceName(any()) } throws networkException

        mocks.verifyAction(setDeviceNameAction) { inner, eventHandler, _ ->
            coVerifySequence {
                // This throws FxaException.Network, we should retry
                inner.setDeviceName("My Phone")
                // This throws again, so the operation fails
                inner.setDeviceName("My Phone")
                eventHandler.onFxaEvent(FxaEvent.DeviceOperationFailed(FxaDeviceOperation.SET_DEVICE_NAME))
            }
            FxaAuthState.Connected()
        }
    }

    @Test
    fun `FxaActionProcessor fails after multiple network errors in a short time`() = runTest {
        val mocks = Mocks.create(FxaAuthState.Connected())
        every {
            mocks.firefoxAccount.setDeviceName(any())
        } throws networkException andThen testLocalDevice

        mocks.verifyAction(setDeviceNameAction) { inner, eventHandler, _ ->
            coVerifySequence {
                // This fails with FxaException.Network, we should retry
                inner.setDeviceName("My Phone")
                // This time it works
                inner.setDeviceName("My Phone")
                eventHandler.onFxaEvent(
                    FxaEvent.DeviceOperationComplete(FxaDeviceOperation.SET_DEVICE_NAME, testLocalDevice),
                )
            }
            FxaAuthState.Connected()
        }

        mocks.actionProcessor.retryLogic.fastForward(29.seconds)
        every {
            mocks.firefoxAccount.setDeviceName(any())
        } throws networkException andThen testLocalDevice

        mocks.verifyAction(setDeviceNameAction) { inner, eventHandler, _ ->
            coVerifySequence {
                // This throws again and the timeout period is still active, we should fail
                inner.setDeviceName("My Phone")
                eventHandler.onFxaEvent(
                    FxaEvent.DeviceOperationFailed(FxaDeviceOperation.SET_DEVICE_NAME),
                )
            }
            FxaAuthState.Connected()
        }
    }

    @Test
    fun `FxaActionProcessor retrys network errors again after a timeout period`() = runTest {
        val mocks = Mocks.create(FxaAuthState.Connected())
        every {
            mocks.firefoxAccount.setDeviceName(any())
        } throws networkException andThen testLocalDevice

        mocks.verifyAction(setDeviceNameAction) { inner, eventHandler, _ ->
            coVerifySequence {
                // This fails with FxaException.Network, we should retry
                inner.setDeviceName("My Phone")
                // This time it works
                inner.setDeviceName("My Phone")
                eventHandler.onFxaEvent(
                    FxaEvent.DeviceOperationComplete(FxaDeviceOperation.SET_DEVICE_NAME, testLocalDevice),
                )
            }
            FxaAuthState.Connected()
        }

        mocks.actionProcessor.retryLogic.fastForward(31.seconds)
        every {
            mocks.firefoxAccount.setDeviceName(any())
        } throws networkException andThen testLocalDevice

        mocks.verifyAction(setDeviceNameAction) { inner, eventHandler, _ ->
            coVerifySequence {
                // Timeout period over, we should retry this time
                inner.setDeviceName("My Phone")
                // This time it works
                inner.setDeviceName("My Phone")
                eventHandler.onFxaEvent(
                    FxaEvent.DeviceOperationComplete(FxaDeviceOperation.SET_DEVICE_NAME, testLocalDevice),
                )
            }
            FxaAuthState.Connected()
        }
    }

    @Test
    fun `FxaActionProcessor calls checkAuthorizationStatus after auth errors`() = runTest {
        val mocks = Mocks.create(FxaAuthState.Connected())
        every {
            mocks.firefoxAccount.setDeviceName(any())
        } throws authException andThen testLocalDevice

        mocks.verifyAction(setDeviceNameAction) { inner, eventHandler, persistState ->
            coVerifySequence {
                // This throws FxaException.Authentication, we should recheck the auth status
                inner.setDeviceName("My Phone")
                // The auth check works
                eventHandler.onFxaEvent(
                    FxaEvent.AuthStateChanged(
                        newState = FxaAuthState.Connected(authCheckInProgress = true),
                        transition = FxaAuthStateTransition.AUTH_CHECK_STARTED,
                    ),
                )
                inner.checkAuthorizationStatus()
                persistState()
                eventHandler.onFxaEvent(
                    FxaEvent.AuthStateChanged(
                        newState = FxaAuthState.Connected(),
                        transition = FxaAuthStateTransition.AUTH_CHECK_SUCCESS,
                    ),
                )
                // .. continue on
                inner.setDeviceName("My Phone")
                eventHandler.onFxaEvent(FxaEvent.DeviceOperationComplete(FxaDeviceOperation.SET_DEVICE_NAME, testLocalDevice))
            }
            FxaAuthState.Connected()
        }
    }

    @Test
    fun `FxaActionProcessor fails after 2 auth errors`() = runTest {
        val mocks = Mocks.create(FxaAuthState.Connected())
        every {
            mocks.firefoxAccount.setDeviceName(any())
        } throws authException

        mocks.verifyAction(setDeviceNameAction) { inner, eventHandler, persistState ->
            coVerifySequence {
                // This throws FxaException.Authentication, we should recheck the auth status
                inner.setDeviceName("My Phone")
                // The auth check works
                eventHandler.onFxaEvent(
                    FxaEvent.AuthStateChanged(
                        newState = FxaAuthState.Connected(authCheckInProgress = true),
                        transition = FxaAuthStateTransition.AUTH_CHECK_STARTED,
                    ),
                )
                inner.checkAuthorizationStatus()
                persistState()
                eventHandler.onFxaEvent(
                    FxaEvent.AuthStateChanged(
                        newState = FxaAuthState.Connected(),
                        transition = FxaAuthStateTransition.AUTH_CHECK_SUCCESS,
                    ),
                )
                // .. but this throws again,
                inner.setDeviceName("My Phone")
                // ..so save the state, transition to disconnected, and make the operation fail
                inner.disconnectFromAuthIssues()
                persistState()
                eventHandler.onFxaEvent(
                    FxaEvent.AuthStateChanged(
                        newState = FxaAuthState.Disconnected(fromAuthIssues = true),
                        transition = FxaAuthStateTransition.AUTH_CHECK_FAILED,
                    ),
                )
                eventHandler.onFxaEvent(FxaEvent.DeviceOperationFailed(FxaDeviceOperation.SET_DEVICE_NAME))
            }
            FxaAuthState.Disconnected(fromAuthIssues = true)
        }
    }

    @Test
    fun `FxaActionProcessor fails if the auth check fails`() = runTest {
        val mocks = Mocks.create(FxaAuthState.Connected())
        every {
            mocks.firefoxAccount.setDeviceName(any())
        } throws authException andThen testLocalDevice

        every { mocks.firefoxAccount.checkAuthorizationStatus() } returns AuthorizationInfo(active = false)

        mocks.verifyAction(setDeviceNameAction) { inner, eventHandler, persistState ->
            coVerify {
                // This throws FxaException.Authentication, we should recheck the auth status
                inner.setDeviceName("My Phone")
                // The auth check fails
                eventHandler.onFxaEvent(
                    FxaEvent.AuthStateChanged(
                        newState = FxaAuthState.Connected(authCheckInProgress = true),
                        transition = FxaAuthStateTransition.AUTH_CHECK_STARTED,
                    ),
                )
                inner.checkAuthorizationStatus()
                inner.disconnectFromAuthIssues()
                persistState()
                eventHandler.onFxaEvent(
                    FxaEvent.AuthStateChanged(
                        newState = FxaAuthState.Disconnected(fromAuthIssues = true),
                        transition = FxaAuthStateTransition.AUTH_CHECK_FAILED,
                    ),
                )
                // .. so the operation fails
                eventHandler.onFxaEvent(FxaEvent.DeviceOperationFailed(FxaDeviceOperation.SET_DEVICE_NAME))
            }
            FxaAuthState.Disconnected(fromAuthIssues = true)
        }
    }

    @Test
    fun `FxaActionProcessor fails after multiple auth errors in a short time`() = runTest {
        val mocks = Mocks.create(FxaAuthState.Connected())
        every {
            mocks.firefoxAccount.setDeviceName(any())
        } throws authException andThen testLocalDevice

        mocks.verifyAction(setDeviceNameAction) { inner, eventHandler, persistState ->
            coVerify {
                // This throws FxaException.Authentication, we should recheck the auth status
                inner.setDeviceName("My Phone")
                // The auth check works
                eventHandler.onFxaEvent(
                    FxaEvent.AuthStateChanged(
                        newState = FxaAuthState.Connected(authCheckInProgress = true),
                        transition = FxaAuthStateTransition.AUTH_CHECK_STARTED,
                    ),
                )
                inner.checkAuthorizationStatus()
                persistState()
                eventHandler.onFxaEvent(
                    FxaEvent.AuthStateChanged(
                        newState = FxaAuthState.Connected(),
                        transition = FxaAuthStateTransition.AUTH_CHECK_SUCCESS,
                    ),
                )
                // .. continue on
                inner.setDeviceName("My Phone")
                eventHandler.onFxaEvent(FxaEvent.DeviceOperationComplete(FxaDeviceOperation.SET_DEVICE_NAME, testLocalDevice))
            }
            FxaAuthState.Connected()
        }

        mocks.actionProcessor.retryLogic.fastForward(59.seconds)
        every {
            mocks.firefoxAccount.setDeviceName(any())
        } throws authException andThen testLocalDevice

        mocks.verifyAction(setDeviceNameAction) { inner, eventHandler, persistState ->
            coVerify {
                // This throws,
                inner.setDeviceName("My Phone")
                // ..so save the state, transition to disconnected, and make the operation fail
                inner.disconnectFromAuthIssues()
                persistState()
                eventHandler.onFxaEvent(
                    FxaEvent.AuthStateChanged(
                        newState = FxaAuthState.Disconnected(fromAuthIssues = true),
                        transition = FxaAuthStateTransition.AUTH_CHECK_FAILED,
                    ),
                )
                eventHandler.onFxaEvent(FxaEvent.DeviceOperationFailed(FxaDeviceOperation.SET_DEVICE_NAME))
            }
            FxaAuthState.Disconnected(fromAuthIssues = true)
        }
    }

    @Test
    fun `FxaActionProcessor checks authorization again after timeout period passes`() = runTest {
        val mocks = Mocks.create(FxaAuthState.Connected())
        every {
            mocks.firefoxAccount.setDeviceName(any())
        } throws authException andThen testLocalDevice

        mocks.verifyAction(setDeviceNameAction) { inner, eventHandler, persistState ->
            coVerify {
                // This throws FxaException.Authentication, we should recheck the auth status
                inner.setDeviceName("My Phone")
                // The auth check works
                eventHandler.onFxaEvent(
                    FxaEvent.AuthStateChanged(
                        newState = FxaAuthState.Connected(authCheckInProgress = true),
                        transition = FxaAuthStateTransition.AUTH_CHECK_STARTED,
                    ),
                )
                inner.checkAuthorizationStatus()
                persistState()
                eventHandler.onFxaEvent(
                    FxaEvent.AuthStateChanged(
                        newState = FxaAuthState.Connected(),
                        transition = FxaAuthStateTransition.AUTH_CHECK_SUCCESS,
                    ),
                )
                // .. continue on
                inner.setDeviceName("My Phone")
                eventHandler.onFxaEvent(FxaEvent.DeviceOperationComplete(FxaDeviceOperation.SET_DEVICE_NAME, testLocalDevice))
            }
            FxaAuthState.Connected()
        }

        mocks.actionProcessor.retryLogic.fastForward(61.seconds)
        every {
            mocks.firefoxAccount.setDeviceName(any())
        } throws authException andThen testLocalDevice

        mocks.verifyAction(setDeviceNameAction) { inner, eventHandler, persistState ->
            coVerify {
                // Timeout period over, we should recheck the auth status this time
                inner.setDeviceName("My Phone")
                // The auth check works
                eventHandler.onFxaEvent(
                    FxaEvent.AuthStateChanged(
                        newState = FxaAuthState.Connected(authCheckInProgress = true),
                        transition = FxaAuthStateTransition.AUTH_CHECK_STARTED,
                    ),
                )
                inner.checkAuthorizationStatus()
                persistState()
                eventHandler.onFxaEvent(
                    FxaEvent.AuthStateChanged(
                        newState = FxaAuthState.Connected(),
                        transition = FxaAuthStateTransition.AUTH_CHECK_SUCCESS,
                    ),
                )
                // .. continue on
                inner.setDeviceName("My Phone")
                eventHandler.onFxaEvent(FxaEvent.DeviceOperationComplete(FxaDeviceOperation.SET_DEVICE_NAME, testLocalDevice))
            }
            FxaAuthState.Connected()
        }
    }

    @Test
    fun `FxaActionProcessor retries after an auth + network exception`() = runTest {
        val mocks = Mocks.create(FxaAuthState.Connected())
        every {
            mocks.firefoxAccount.setDeviceName(any())
        } throws authException andThen testLocalDevice

        every {
            mocks.firefoxAccount.checkAuthorizationStatus()
        } throws networkException andThen AuthorizationInfo(active = true)

        mocks.verifyAction(setDeviceNameAction) { inner, eventHandler, persistState ->
            coVerify {
                // This throws FxaException.Authentication, we should recheck the auth status
                inner.setDeviceName("My Phone")
                eventHandler.onFxaEvent(
                    FxaEvent.AuthStateChanged(
                        newState = FxaAuthState.Connected(authCheckInProgress = true),
                        transition = FxaAuthStateTransition.AUTH_CHECK_STARTED,
                    ),
                )
                // This throws a network error, we should retry
                inner.checkAuthorizationStatus()
                // This time it works
                inner.checkAuthorizationStatus()
                persistState()
                eventHandler.onFxaEvent(
                    FxaEvent.AuthStateChanged(
                        newState = FxaAuthState.Connected(),
                        transition = FxaAuthStateTransition.AUTH_CHECK_SUCCESS,
                    ),
                )
                // .. continue on
                inner.setDeviceName("My Phone")
                eventHandler.onFxaEvent(FxaEvent.DeviceOperationComplete(FxaDeviceOperation.SET_DEVICE_NAME, testLocalDevice))
            }
            FxaAuthState.Connected()
        }
    }

    @Test
    fun `FxaActionProcessor retries after a network + auth exception`() = runTest {
        val mocks = Mocks.create(FxaAuthState.Connected())
        every {
            mocks.firefoxAccount.setDeviceName(any())
        } throws authException andThen testLocalDevice

        every {
            mocks.firefoxAccount.checkAuthorizationStatus()
        } throws networkException andThen AuthorizationInfo(active = true)

        mocks.verifyAction(setDeviceNameAction) { inner, eventHandler, persistState ->
            coVerify {
                // This throws FxaException.Network should try again
                inner.setDeviceName("My Phone")
                // This throws FxaException.Authentication, we should recheck the auth status
                inner.setDeviceName("My Phone")
                eventHandler.onFxaEvent(
                    FxaEvent.AuthStateChanged(
                        newState = FxaAuthState.Connected(authCheckInProgress = true),
                        transition = FxaAuthStateTransition.AUTH_CHECK_STARTED,
                    ),
                )
                // This works
                inner.checkAuthorizationStatus()
                persistState()
                eventHandler.onFxaEvent(
                    FxaEvent.AuthStateChanged(
                        newState = FxaAuthState.Connected(),
                        transition = FxaAuthStateTransition.AUTH_CHECK_SUCCESS,
                    ),
                )
                // .. continue on
                inner.setDeviceName("My Phone")
                eventHandler.onFxaEvent(FxaEvent.DeviceOperationComplete(FxaDeviceOperation.SET_DEVICE_NAME, testLocalDevice))
            }
            FxaAuthState.Connected()
        }
    }
}
