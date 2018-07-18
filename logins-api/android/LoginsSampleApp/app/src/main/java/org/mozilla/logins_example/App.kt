package org.mozilla.logins_example
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
        if (acct == null || acct.creds == null) {
            intent = Intent(baseContext, LoginActivity::class.java)
        } else {
            intent = Intent(baseContext, MainActivity::class.java)
        }
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
