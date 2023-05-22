/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * This is a mock implementation of Android's `Drawable` object to allow us to run tests of the generated
 * code against the real `FeatureVariables` code.
 */
@file:Suppress("InvalidPackageDeclaration")

package android.graphics.drawable

@Suppress("PACKAGE_OR_CLASSIFIER_REDECLARATION")
class Drawable(val res: Int)
