package org.mozilla.logins_example

import android.net.Uri
import android.support.v7.app.AppCompatActivity
import android.os.Bundle
import android.webkit.*

import io.github.mozilla.sandvich.rust.Config
import io.github.mozilla.sandvich.rust.FirefoxAccount

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

        val config = Config.custom(contentBase)

        val fxa = FirefoxAccount(config!!, clientId)
        val scopes = arrayOf("profile",
                             "https://identity.mozilla.com/apps/oldsync",
                             "https://identity.mozilla.com/apps/lockbox"
        )
        val flowUrl = fxa.beginOAuthFlow(redirectUri, scopes, true)

        val wv: WebView =  findViewById(R.id.webview)
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
                    val oauthInfo = fxa.completeOAuthFlow(code, state)
                    val profile = fxa.profile

                    val creds = Credentials(oauthInfo.accessToken, oauthInfo.keys, fxa.tokenServerEndpointURL)
                    ExampleApp.instance.account = Account(profile.email, creds)
                    ExampleApp.instance.startNewIntent()
                    this@LoginActivity.finish()
                    return true
                }
                wv.loadUrl(url)
                return true
            }
        }
        wv.loadUrl(flowUrl)
    }
}
