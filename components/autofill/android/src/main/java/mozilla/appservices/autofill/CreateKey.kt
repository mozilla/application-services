/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.autofill

// We needed to rename the the autofill `createKey` function as it conflicts with the `createKey` function we
// are exposing for Logins in iOS. However renaming `createKey` to `createAutofillKey` creates a breaking change
// for the Android code. So we are aliasing `createAutofillKey` back to `createKey` in order to prevent the
// breaking change.

fun createKey() = createAutofillKey()
