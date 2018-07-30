---
id: fxa-client-ios
title: iOS SDK
---

The **Firefox Accounts iOS SDK** provides a way for iOS applications to do the following:

* Fetch scoped keys to provide a key for end-to-end encryption.
* Get access to Firefox Sync keys to fetch the sync data.
* Fetch user's profile to personalize the applications.

Please see the [FxA iOS SDK documentation](https://github.com/mozilla/application-services/tree/master/fxa-rust-client/sdks/swift/FxAClient)
to integrate this component into your application.

## Implementing the OAuth flow in iOS

> This tutorial is for FxAClient iOS 0.2.0.


### Setup Environment 

First you need some OAuth information. Generate a `client_id`, `redirectUrl` and find out the scopes for your application.
See Firefox Account documentation for that. 

Once you have the OAuth info, you can start adding `FxAClient` to your iOS project.
As part of the OAuth flow your application will be opening up a Web view or open the system browser.
Currently the SDK does not provide the Web view, you have to write it yourself.

We use Carthage to distribute this library. Add the following to your `Cartfile`:

```
github "mozilla/application-services" "0.2.0"
```

After that run `carthage update`, this will download the prebuilt components.

> If you do not use Carthage then you will have to build the library from source. This is 
not recommended. 


### Start coding

Importing the `FxAClient`:

```swift
import FxAClient
```

Create a global `fxa` object: 

```swift
var fxa: FirefoxAccount?
```

You will need to save state for FxA in your app, this example just uses `UserDefaults`. We suggest using the iOS key store for this data.
Define `self` variables to help save state for FxA:

```swift
self.stateKey = "fxaState"
self.redirectUrl = "https://mozilla-lockbox.github.io/fxa/ios-redirect.html"
```

Then you can write the following:

```swift
if let state_json = UserDefaults.standard.string(forKey: self.stateKey) {
    fxa = try! FirefoxAccount.fromJSON(state: state_json)
} else {
    FxAConfig.custom(content_base: "https://accounts.firefox.com") { result, error in
        guard let config = result else { return }
        fxa = try! FirefoxAccount(config: config, clientId: "[YOUR_CLIENT_ID]")
    }
}
```

The code above checks if you have some existing state for FxA, otherwise it configures it.

You can now attempt to fetch the FxA profile. The first time the application starts it won't have any state, so
`fxa.getProfile()` will fail and proceed to the `fxa.beginOAuthFlow` branch and it will open the FxA OAuth login
in the web view.

```swift
fxa.getProfile() { result, error in
    if let error = error as? FxAError, case FxAError.Unauthorized = error {
        fxa.beginOAuthFlow(scopes: ["profile", "https://identity.mozilla.com/apps/oldsync"], wantsKeys: true) { result, error in
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
```

```swift
func matchingRedirectURLReceived(components: URLComponents) {
    var dic = [String: String]()
    components.queryItems?.forEach { dic[$0.name] = $0.value }
    self.fxa!.completeOAuthFlow(code: dic["code"]!, state: dic["state"]!) { result, error in
        guard let oauthInfo = result else { return }
        print("access_token: " + oauthInfo.accessToken)
        if let keys = oauthInfo.keys {
            print("keysJWE: " + keys)
        }
        print("obtained scopes: " + oauthInfo.scopes.joined(separator: " "))
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
```

