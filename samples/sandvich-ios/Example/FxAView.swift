/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import WebKit
import UIKit
import FxAClient

let stateKey = "fxaState"

// NOTE: This is the client ID for "Android Components Reference Browser".
// This is wrong, but it works for now :-/
private let ClientID = "3c49430b43dfba77"

private let RedirectURL = "https://accounts.firefox.com/oauth/success/\(ClientID)"

private let ProfileScope = "https://identity.mozilla.com/apps/oldsync"

class FxAView: UIViewController, WKNavigationDelegate {
    private var webView: WKWebView
    var fxa: FirefoxAccount?

    override var preferredStatusBarStyle: UIStatusBarStyle {
        return UIStatusBarStyle.lightContent
    }

    init(webView: WKWebView = WKWebView()) {
        self.webView = webView
        super.init(nibName: nil, bundle: nil)
    }

    func tryGetProfile() {
        if let fxa = self.fxa {
            fxa.getProfile() { result, error in
                if let error = error as? FxAError, case FxAError.Unauthorized = error {
                    fxa.beginOAuthFlow(scopes: ["profile", ProfileScope], wantsKeys: true) { result, error in
                        guard let authUrl = result else { return }
                        DispatchQueue.main.async {
                            self.webView.load(URLRequest(url: authUrl))
                        }
                    }
                } else if let profile = result {
                    DispatchQueue.main.async {
                        self.navigationController?.pushViewController(ProfileView(email: profile.email), animated: true)
                    }
                } else {
                    assert(false, "Unexpected error :(")
                }
            }
        }
    }

    class EzPersistor: PersistCallback {
        func persist(json: String) {
            UserDefaults.standard.set(json, forKey: stateKey)
        }
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        self.webView.navigationDelegate = self
        self.view = self.webView
        self.styleNavigationBar()

        let persistor = EzPersistor()

        if let state_json = UserDefaults.standard.string(forKey: stateKey) {
            self.fxa = try! FirefoxAccount.fromJSON(state: state_json)
            persistor.persist(json: (try! fxa?.toJSON())!) // Persist the FxA state right after its creation in case something goes wrong.
            fxa!.registerPersistCallback(persistor) // After this, mutating changes will be persisted automatically.
            self.tryGetProfile()
        } else {
            let config = FxAConfig.release(clientId: ClientID, redirectUri: RedirectURL)
            self.fxa = try! FirefoxAccount(config: config)
            persistor.persist(json: (try! self.fxa?.toJSON())!)
            self.fxa!.registerPersistCallback(persistor)
            self.tryGetProfile()
        }
    }


    func webViewRequest(decidePolicyFor navigationAction: WKNavigationAction,
                        decisionHandler: @escaping (WKNavigationActionPolicy) -> Void) {
        if let navigationURL = navigationAction.request.url {
            let expectedRedirectURL = URL(string: RedirectURL)!
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
        self.fxa!.completeOAuthFlow(code: dic["code"]!, state: dic["state"]!) { result, error in
            self.fxa!.getAccessToken(scope: ProfileScope) { result, error in
                guard let tokenInfo = result else { return }
                print("access_token: " + tokenInfo.token)
                if let key = tokenInfo.key {
                    print("key: " + key)
                }
                self.fxa!.getProfile() { result, error in
                    guard let profile = result else {
                        assert(false, "ok something's really wrong there")
                        return
                    }
                    DispatchQueue.main.async {
                        self.navigationController?.pushViewController(ProfileView(email: profile.email), animated: true)
                    }
                }
            }
        }
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
