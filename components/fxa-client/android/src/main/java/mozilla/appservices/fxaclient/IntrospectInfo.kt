/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.fxaclient

data class IntrospectInfo(
    val active: Boolean
) {
    companion object {
        internal fun fromMessage(msg: MsgTypes.IntrospectInfo): IntrospectInfo {
            return IntrospectInfo(
                active = msg.active
            )
        }
    }
}
