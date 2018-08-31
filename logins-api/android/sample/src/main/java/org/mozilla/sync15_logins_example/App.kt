/* Copyright 2018 Mozilla
 * Licensed under the Apache License, Version 2.0 (the "License"); you may not use
 * this file except in compliance with the License. You may obtain a copy of the
 * License at http://www.apache.org/licenses/LICENSE-2.0
 * Unless required by applicable law or agreed to in writing, software distributed
 * under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
 * CONDITIONS OF ANY KIND, either express or implied. See the License for the
 * specific language governing permissions and limitations under the License. */
package org.mozilla.sync15_logins_example
import android.app.Application
import android.content.Context
import android.content.Intent

// The credentials the login flow gave us.
class Credentials(
    val accessToken: String,
    val keys: String,
    val tokenServer: String // not strictly credentials, but that's ok...
)

// The account - the intention is that we could drop creds while keeping the email when we detect
// the account is in a bad auth state.
class Account(val email: String, val creds: Credentials?)

class ExampleApp: Application() {
    companion object {
        private var singleton: ExampleApp? = null
        val instance: ExampleApp
            get() {
                return singleton!!
            }
    }

    override fun onCreate() {
        super.onCreate()
        singleton = this
    }

    fun startNewIntent() {
        val acct = account
        val intent: Intent
        if (acct?.creds == null) {
            intent = Intent(baseContext, LoginActivity::class.java)
        } else {
            intent = Intent(baseContext, MainActivity::class.java)
        }
        intent.flags = Intent.FLAG_ACTIVITY_NEW_TASK
        startActivity(intent)
    }

    var account: Account?
        get() {
            val prefs = applicationContext.getSharedPreferences(
                    getString(R.string.preference_file_key), Context.MODE_PRIVATE)
            val email = prefs.getString("email", null)
            if (email.isNullOrEmpty()) {
                return null
            }
            val accessToken = prefs.getString("accessToken",null)
            val keys = prefs.getString("keys", null)
            val tokenServer = prefs.getString("tokenServer", null)
            val creds = if (accessToken.isNullOrEmpty() ||
                            keys.isNullOrEmpty() ||
                            tokenServer.isNullOrEmpty()) null
                        else Credentials(accessToken, keys, tokenServer)
            return Account(email, creds)
        }

        set(acct) {
            applicationContext.getSharedPreferences(
                getString(R.string.preference_file_key), Context.MODE_PRIVATE)
                .edit().apply {
                    if (acct == null) {
                        remove("email")
                        remove("accessToken")
                        remove("keys")
                        remove("tokenServer")
                    } else {
                        putString("email", acct.email)
                        if (acct.creds == null) {
                            remove("accessToken")
                            remove("keys")
                            remove("tokenServer")
                        } else {
                            putString("accessToken", acct.creds.accessToken)
                            putString("keys", acct.creds.keys)
                            putString("tokenServer", acct.creds.tokenServer)
                        }
                    }
                    apply()
                }
        }
}
