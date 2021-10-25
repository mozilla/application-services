/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * This is a mock implementation of Android's `Context` object to allow us to run tests of the generated
 * code against the real `FeatureVariables` code.
 */
package android.content;

@Suppress("UNUSED_PARAMETER", "PACKAGE_OR_CLASSIFIER_REDECLARATION")
class Context {
    val resources = Resources

    val packageName = "dummy.package.name"

    fun getDrawable(res: Int): android.graphics.drawable.Drawable? = null

    fun getString(res: Int): String? = null
}

@Suppress("UNUSED_PARAMETER", "PACKAGE_OR_CLASSIFIER_REDECLARATION")
object Resources {
    fun getIdentifier(resName: String, defType: String, packageName: String): Int? = null
}