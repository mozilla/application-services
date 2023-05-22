/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

@file:Suppress("InvalidPackageDeclaration")

package mozilla.appservices.remotetabs

// We needed to rename the Rust `RemoteTab` struct to `RemoteTabRecord` in order to circumvent the naming conflict in
// iOS with the native `RemoteTab` struct. But that creates a breaking change for the Android code. So we are aliasing
// `RemoteTabRecord` back to `RemoteTab` to prevent a breaking change.
typealias RemoteTab = RemoteTabRecord
