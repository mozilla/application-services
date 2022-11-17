/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

@file:Suppress("InvalidPackageDeclaration")
package mozilla.appservices.remotetabs

// We needed to rename the tabs `DeviceType` as it conflicts with the `DeviceType` we are exposing for FxA
// in iOS. However renaming `DeviceType` to `TabsDeviceType` creates a breaking change for the Android code.
// So we are aliasing `TabsDeviceType` back to `DeviceType` in order to prevent the breaking change.
typealias DeviceType = TabsDeviceType
