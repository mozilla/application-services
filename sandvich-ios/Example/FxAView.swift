/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import WebKit
import UIKit
import FxAClient

class FxAView: UIViewController, WKNavigationDelegate {
    private var webView: WKWebView
    var redirectUrl: String
    var stateKey: String
    var fxa: FirefoxAccount?

    override var preferredStatusBarStyle: UIStatusBarStyle {
        return UIStatusBarStyle.lightContent
    }

    init(webView: WKWebView = WKWebView()) {
        self.webView = webView
        self.stateKey = "fxaState"
        self.redirectUrl = "com.mozilla.sandvich:/oauth2redirect/fxa-provider"
        super.init(nibName: nil, bundle: nil)
    }

    func persistState(_ fxa: FirefoxAccount) {
        UserDefaults.standard.set(fxa.toJSON()!, forKey: self.stateKey)
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        self.webView.navigationDelegate = self
        self.view = self.webView
        self.styleNavigationBar()

        let fxa: FirefoxAccount;
        if let state_json = UserDefaults.standard.string(forKey: self.stateKey) {
            fxa = FirefoxAccount.fromJSON(state: state_json)
        } else {
            let config = FxAConfig.custom(content_base: "https://sandvich-ios.dev.lcip.org");
            fxa = FirefoxAccount(config: config, clientId: "22d74070a481bc73")
            persistState(fxa)
        }

        if let profile = fxa.profile { // If we aren't able to get the profile, we'll try to get a token (we should probably check errors in the future).
            self.navigationController?.pushViewController(ProfileView(email: profile.email), animated: true)
        }
        self.fxa = fxa
        let authUrl = fxa.beginOAuthFlow(redirectURI: self.redirectUrl, scopes: ["profile", "https://identity.mozilla.com/apps/oldsync"], wantsKeys: true)!
        self.webView.load(URLRequest(url: authUrl))
    }


    func webViewRequest(decidePolicyFor navigationAction: WKNavigationAction,
                        decisionHandler: @escaping (WKNavigationActionPolicy) -> Void) {
        if let navigationURL = navigationAction.request.url {
            let expectedRedirectURL = URL(string: redirectUrl)!
            if navigationURL.scheme == expectedRedirectURL.scheme && navigationURL.host == expectedRedirectURL.host && navigationURL.path == expectedRedirectURL.path,
                let components = URLComponents(url: navigationURL, resolvingAgainstBaseURL: true) {
                matchingRedirectURLReceived(components: components)
                decisionHandler(.cancel)
                return
            }
        }

        decisionHandler(.allow)
    }

    func matchingRedirectURLReceived(components: URLComponents) {
        var dic = [String: String]()
        components.queryItems?.forEach { dic[$0.name] = $0.value }
        let oauthInfo = self.fxa!.completeOAuthFlow(code: dic["code"]!, state: dic["state"]!)!
        persistState(self.fxa!) // Persist fxa state because it now holds the profile token.
        print("access_token: " + oauthInfo.accessToken)
        if let keys = oauthInfo.keysJWE {
            print("keysJWE: " + keys)
        }
        print("obtained scopes: " + oauthInfo.scopes.joined(separator: " "))
        guard let profile = fxa!.profile else {
            print("Something is super wrong")
            assert(false)
        }
        self.navigationController?.pushViewController(ProfileView(email: profile.email), animated: true)
        return
    }

    func webView(_ webView: WKWebView,
                 decidePolicyFor navigationAction: WKNavigationAction,
                 decisionHandler: @escaping (WKNavigationActionPolicy) -> Void) {
        webViewRequest(decidePolicyFor: navigationAction, decisionHandler: decisionHandler)
    }

    private func styleNavigationBar() {
        self.navigationItem.leftBarButtonItem = UIBarButtonItem(
            title: "Cancel",
            style: .plain,
            target: nil,
            action: nil
        )

        self.navigationItem.leftBarButtonItem!.setTitleTextAttributes([
            NSAttributedStringKey.foregroundColor: UIColor.white,
            NSAttributedStringKey.font: UIFont.systemFont(ofSize: 18, weight: .semibold)
            ], for: .normal)

        if #available(iOS 11.0, *) {
            self.navigationItem.largeTitleDisplayMode = .never
        }
    }

    required init?(coder aDecoder: NSCoder) {
        fatalError("not implemented")
    }
}
