/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

@file:Suppress("InvalidPackageDeclaration")

package org.mozilla.experiments.nimbus.internal

data class GeckoPref(
    var `pref`: kotlin.String,
    var `branch`: PrefBranch,
)

enum class PrefBranch {
    DEFAULT,
    USER,
}
