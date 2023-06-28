/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.components.service.fxa

import mozilla.appservices.fxaclient.Device
import mozilla.appservices.fxaclient.DeviceCapability
import mozilla.appservices.fxaclient.DevicePushSubscription
import mozilla.appservices.fxaclient.IncomingDeviceCommand
import mozilla.appservices.sync15.DeviceType

/**
 * Manages the list of connected devices for a [FirefoxAccount]
 */
abstract class DeviceManager {
    /**
     * Set name of the current device.
     * @param name New device name.
     * @throws FxaException.Authentication if the account is not connected
     */
    abstract suspend fun setDeviceName(name: String)

    /**
     * Get the current device list. May be incomplete if state was never queried.
     * @return [DeviceList] describes current and other known devices
     * @throws FxaException.Authentication if the account is not connected
     */
    abstract fun getKnownDevices(): DeviceList

    /**
     * Set a [DevicePushSubscription] for the current device.
     * @param subscription A new [DevicePushSubscription].
     * @throws FxaException.Authentication if the account is not connected
     */
    abstract suspend fun setDevicePushSubscription(subscription: DevicePushSubscription)

    /**
     * Send a command to a specified device.
     * @param targetDeviceId A device ID of the recipient.
     * @param title tab title
     * @param url tab URL
     * @throws FxaException.Authentication if the account is not connected
     */
    abstract suspend fun sendSingleTab(targetDeviceId: String, title: String, url: String)

    /**
     * Refreshes [DeviceList]. Registered [AccountEventsObserver] observers will be notified.
     *
     * @throws FxaException.Authentication if the account is not connected
     */
    abstract suspend fun refreshDevices()

    /**
     * Polls for any pending [DeviceCommandIncoming] commands.
     * In case of new commands, registered [AccountEventsObserver] observers will be notified.
     *
     * @throws FxaException.Authentication if the account is not connected
     */
    abstract suspend fun pollForDeviceCommands()
}

/**
 * Configuration for the current device.
 *
 * @property name An initial name to use for the device record which will be created during authentication.
 * This can be changed later via [setDeviceName].
 * @property type Type of a device - mobile, desktop - used for displaying identifying icons on other devices.
 * This cannot be changed once device record is created.
 * @property capabilities A set of device capabilities, such as SEND_TAB.
 * @property secureStateAtRest A flag indicating whether or not to use encrypted storage for the persisted account
 * state.
 */
data class DeviceConfig(
    val name: String,
    val type: DeviceType,
    val capabilities: Set<DeviceCapability>,
    val secureStateAtRest: Boolean = false,
)

/**
 * Describes current device and other known devices.
 */
data class DeviceList(val currentDevice: Device, val otherDevices: List<Device>)
