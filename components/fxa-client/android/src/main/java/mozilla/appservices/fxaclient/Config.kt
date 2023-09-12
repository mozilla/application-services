/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.fxaclient

fun FxaServer.isCustom() = this is FxaServer.Custom

fun FxaServer.contentUrl() = when (this) {
    is FxaServer.Release -> "https://accounts.firefox.com"
    is FxaServer.Stable -> "https://stable.dev.lcip.org"
    is FxaServer.Stage -> "https://accounts.stage.mozaws.net"
    is FxaServer.China -> "https://accounts.firefox.com.cn"
    is FxaServer.LocalDev -> "http://127.0.0.1:3030"
    is FxaServer.Custom -> this.url
}
