/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

@file:Suppress("InvalidPackageDeclaration")

package mozilla.appservices.remotetabs

// We needed to rename the Rust `TabsGuid` struct to `Guid` in order to circumvent the naming conflict in
// iOS with the Guid exposed in `places.udl`. But that creates a breaking change for the Android code. So we are aliasing
// `TabsGuid` back to `Guid` to prevent a breaking change.
typealias Guid = TabsGuid
