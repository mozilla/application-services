/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.fxaclient

data class TabHistoryEntry(
    val title: String,
    val url: String
)

// https://proandroiddev.com/til-when-is-when-exhaustive-31d69f630a8b
val <T> T.exhaustive: T
    get() = this

sealed class AccountEvent {
    // A tab with all its history entries (back button).
    class TabReceived(val from: Device?, val entries: Array<TabHistoryEntry>) : AccountEvent()

    companion object {
        private fun fromMessage(msg: MsgTypes.AccountEvent): AccountEvent {
            return when (msg.type) {
                MsgTypes.AccountEvent.AccountEventType.TAB_RECEIVED -> {
                    val data = msg.tabReceivedData
                    TabReceived(
                        from = if (data.hasFrom()) Device.fromMessage(data.from) else null,
                        entries = data.entriesList.map {
                            TabHistoryEntry(title = it.title, url = it.url)
                        }.toTypedArray()
                    )
                }
                null -> throw NullPointerException("AccountEvent type cannot be null.")
            }.exhaustive
        }
        internal fun fromCollectionMessage(msg: MsgTypes.AccountEvents): Array<AccountEvent> {
            return msg.eventsList.map {
                fromMessage(it)
            }.toTypedArray()
        }
    }
}
