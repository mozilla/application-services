/* Copyright 2018 Mozilla
 * Licensed under the Apache License, Version 2.0 (the "License"); you may not use
 * this file except in compliance with the License. You may obtain a copy of the
 * License at http://www.apache.org/licenses/LICENSE-2.0
 * Unless required by applicable law or agreed to in writing, software distributed
 * under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
 * CONDITIONS OF ANY KIND, either express or implied. See the License for the
 * specific language governing permissions and limitations under the License. */
package org.mozilla.logins_example

import android.net.Uri
import android.support.v7.app.AppCompatActivity
import android.os.Bundle
import android.util.Log
import android.webkit.*
import mozilla.components.service.fxa.Config
import mozilla.components.service.fxa.FirefoxAccount
import mozilla.components.service.fxa.OAuthInfo

// In *theory*, we could change this to be any of the FxA stacks (eg, stage, dev, sandvich, etc)
// although the flies in the ointment are that the clientId may not be valid in all of them, and
// the redirectUri may be different for all of them (and it is typically enforced).
// In theory, there's a sekrit URL which @rfkelly can tell you that will probably allow you to look
// up the redirect URL given a clientId (and as a side-effect ensure the clientId is valid on that
// stack), but that doesn't seem worthwhile to do at this stage.

// So these values are valid for the prod stack.
const val contentBase = "https://accounts.firefox.com"
const val clientId = "98adfa37698f255b"
const val redirectUri = "https://lockbox.firefox.com/fxa/ios-redirect.html"

class LoginActivity : AppCompatActivity() {
    companion object {
        init {
            // We need to pre-load some of these libraries or fxa_client tries to load them with strange
            // names (eg, lib_sso.1.1.0.so or something...)
            System.loadLibrary("crypto")
            System.loadLibrary("ssl")
            System.loadLibrary("fxa_client")
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_login)
        var fxa: FirefoxAccount? = null;
        Config.custom(contentBase).then({ value: Config ->
            fxa = FirefoxAccount(value, clientId, redirectUri)
            var scopes = arrayOf(
                    "profile",
                    "https://identity.mozilla.com/apps/oldsync",
                    "https://identity.mozilla.com/apps/lockbox"
            );

            fxa!!.beginOAuthFlow(scopes, true)
        }, { err ->
            Log.e("Logins", "(begin oauth): ", err);
            throw err;
        }).whenComplete { flowUrl: String ->
            // XXX Hack: FxA prod doesn't accept + in urls but fxa-client leaves them in for now...
//            val fixedUrl = flowUrl.replace("+", "%20");
            Log.d("Logins", "Flow URL: " + flowUrl)
            runOnUiThread {
                showWebView(flowUrl, fxa!!)
            }
        }
    }

    private fun showWebView(flowUrl: String, fxa: FirefoxAccount) {
        val wv: WebView = findViewById(R.id.webview)
        // Need JS, cookies and localStorage.
        wv.settings.domStorageEnabled = true
        wv.settings.javaScriptEnabled = true
        CookieManager.getInstance().setAcceptCookie(true)

        wv.webViewClient = object : WebViewClient() {
            override fun shouldOverrideUrlLoading(view: WebView?, request: WebResourceRequest?): Boolean {
                val uri: Uri = request?.url!!
                val url = uri.toString()
                if (url.startsWith(redirectUri)) {
                    // we are done!
                    val code = uri.getQueryParameter("code")
                    val state = uri.getQueryParameter("state")
                    var oauthInfo: OAuthInfo? = null;
                    fxa.completeOAuthFlow(code, state).then { info ->
                        oauthInfo = info;
                        fxa.getProfile()
                    }.whenComplete { profile ->
                        val creds = Credentials(oauthInfo!!.accessToken!!, oauthInfo!!.keys!!, fxa.getTokenServerEndpointURL()!!)
                        ExampleApp.instance.account = Account(profile.email!!, creds)
                        ExampleApp.instance.startNewIntent()
                        this@LoginActivity.finish()
                    }
                    return true;
                }
                wv.loadUrl(url)
                return true
            }
        }
        wv.loadUrl(flowUrl)
    }
}
