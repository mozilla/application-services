/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.accounts

import android.content.Context
import android.content.SharedPreferences

const val FXA_STATE_PREFS_KEY = "fxaAppState"
const val FXA_STATE_KEY = "fxaState"

interface AccountStorage {
    @Throws(Exception::class)
    fun read(): String?
    fun write(accountState: String)
    fun clear()
}

class SharedPrefAccountStorage(val context: Context) : AccountStorage {
    override fun read(): String? {
        return accountPreferences().getString(FXA_STATE_KEY, null)
    }

    override fun write(accountState: String) {
        accountPreferences()
            .edit()
            .putString(FXA_STATE_KEY, accountState)
            .apply()
    }

    override fun clear() {
        accountPreferences()
            .edit()
            .remove(FXA_STATE_KEY)
            .apply()
    }

    private fun accountPreferences(): SharedPreferences {
        return context.getSharedPreferences(FXA_STATE_PREFS_KEY, Context.MODE_PRIVATE)
    }
}
