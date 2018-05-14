/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import WebKit
import UIKit


class FxAView: UIViewController, WKNavigationDelegate {
    private var webView: WKWebView
    
    override var preferredStatusBarStyle: UIStatusBarStyle {
        return UIStatusBarStyle.lightContent
    }
    
    init(webView: WKWebView = WKWebView()) {
        self.webView = webView
        super.init(nibName: nil, bundle: nil)
    }
    
    override func viewDidLoad() {
        super.viewDidLoad()
        self.webView.navigationDelegate = self
        self.view = self.webView
        self.styleNavigationBar()
        
        let url = URL(string: "https://google.com")!;
        self.webView.load(URLRequest(url: url))
    }
    

    func webViewRequest(decidePolicyFor navigationAction: WKNavigationAction,
                        decisionHandler: @escaping (WKNavigationActionPolicy) -> Void) {
        if let navigationURL = navigationAction.request.url {
            if "\(navigationURL.scheme!)://\(navigationURL.host!)\(navigationURL.path)" == "https://mozilla-lockbox.github.io/fxa/ios-redirect.html",
                let components = URLComponents(url: navigationURL, resolvingAgainstBaseURL: true) {
                matchingRedirectURLReceived(components: components)
                decisionHandler(.cancel)
                return
            }
        }
        
        decisionHandler(.allow)
    }
    
    func matchingRedirectURLReceived(components: URLComponents) {
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
        
//        if let presenter = self.presenter {
//            self.navigationItem.leftBarButtonItem!.rx.tap
//                .bind(to: presenter.onCancel)
//                .disposed(by: self.disposeBag)
//        }
    }
    
    required init?(coder aDecoder: NSCoder) {
        fatalError("not implemented")
    }
}
