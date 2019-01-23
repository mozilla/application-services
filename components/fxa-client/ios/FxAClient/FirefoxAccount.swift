/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation
import UIKit

// We use a serial queue to protect access to the rust object.
let queue = DispatchQueue(label: "com.fxaclient")

open class FxAConfig {
    public enum Server: String {
        case Release = "https://accounts.firefox.com"
        case Stable = "https://stable.dev.lcip.org"
        case Dev = "https://accounts.stage.mozaws.net"
    }

    let contentUrl: String
    let clientId: String
    let redirectUri: String

    public init(contentUrl: String, clientId: String, redirectUri: String) {
        self.contentUrl = contentUrl
        self.clientId = clientId
        self.redirectUri = redirectUri
    }

    public init(withServer server: Server, clientId: String, redirectUri: String) {
        self.contentUrl = server.rawValue
        self.clientId = clientId
        self.redirectUri = redirectUri
    }

    public static func release(clientId: String, redirectUri: String) -> FxAConfig {
        return FxAConfig.init(withServer: FxAConfig.Server.Release, clientId: clientId, redirectUri: redirectUri)
    }

    public static func stable(clientId: String, redirectUri: String) -> FxAConfig {
        return FxAConfig.init(withServer: FxAConfig.Server.Stable, clientId: clientId, redirectUri: redirectUri)
    }

    public static func dev(clientId: String, redirectUri: String) -> FxAConfig {
        return FxAConfig.init(withServer: FxAConfig.Server.Dev, clientId: clientId, redirectUri: redirectUri)
    }
}

public protocol PersistCallback {
    func persist(json: String)
}

open class FirefoxAccount: RustHandle {
    fileprivate static var persistCallback: PersistCallback?

    #if BROWSERID_FEATURES
    /// Creates a `FirefoxAccount` instance from credentials obtained with the onepw FxA login flow.
    /// This is typically used by the legacy Sync clients: new clients mainly use OAuth flows and
    /// therefore should use `init`.
    /// Please note that the `FxAConfig` provided will be consumed and therefore
    /// should not be re-used.
    open class func from(config: FxAConfig, webChannelResponse: String) throws -> FirefoxAccount {
        return try queue.sync(execute: {
            let handle = try FxAError.unwrap({err in
                fxa_from_credentials(config.contentUrl, config.clientId, config.redirectUri, webChannelResponse, err)
            })
            return FirefoxAccount(raw: handle)
        })
    }
    #endif

    /// Restore a previous instance of `FirefoxAccount` from a serialized state (obtained with `toJSON(...)`).
    open class func fromJSON(state: String) throws -> FirefoxAccount {
        return try queue.sync(execute: {
            let handle = try FxAError.unwrap({ err in fxa_from_json(state, err) })
            return FirefoxAccount(raw: handle)
        })
    }

    /// Create a `FirefoxAccount` from scratch. This is suitable for callers using the
    /// OAuth Flow.
    /// Please note that the `FxAConfig` provided will be consumed and therefore
    /// should not be re-used.
    public convenience init(config: FxAConfig) throws {
        let pointer = try queue.sync(execute: {
            return try FxAError.unwrap({err in
                fxa_new(config.contentUrl, config.clientId, config.redirectUri, err)
            })
        })
        self.init(raw: pointer)
    }

    override func cleanup(pointer: UInt64) {
        queue.sync(execute: {
            try! FxAError.unwrap({err in
                // Is this the right thing to do? We should only hit an error here
                // for panics and handle misuse, both inidicate bugs in our code
                // (the first in the rust code, the 2nd in this swift wrapper).
                fxa_free(pointer, err)
            })
        })
    }

    /// Serializes the state of a `FirefoxAccount` instance. It can be restored later with `fromJSON(...)`.
    /// It is the responsability of the caller to persist that serialized state regularly (after operations that mutate `FirefoxAccount`) in a **secure** location.
    open func toJSON() throws -> String {
        return try queue.sync(execute: {
            return String(freeingFxaString: try FxAError.unwrap({err in
                fxa_to_json(self.raw, err)
            }))
        })
    }

    /// Registers a persistance callback. The callback will get called everytime
    /// the `FirefoxAccount` state needs to be saved. The callback must
    /// persist the passed string in a secure location (like the keychain).
    public func registerPersistCallback(_ cb: PersistCallback) throws {
        FirefoxAccount.persistCallback = cb
        try FxAError.unwrap({err in
            fxa_register_persist_callback(self.raw, persistCallbackFunction, err)
        })
    }

    /// Unregisters a persistance callback.
    public func unregisterPersistCallback() throws {
        FirefoxAccount.persistCallback = nil
        try FxAError.unwrap({err in
            fxa_unregister_persist_callback(self.raw, err)
        })
    }

    /// Gets the logged-in user profile.
    /// Throws `FxAError.Unauthorized` if we couldn't find any suitable access token
    /// to make that call. The caller should then start the OAuth Flow again with
    /// the "profile" scope.
    open func getProfile(completionHandler: @escaping (Profile?, Error?) -> Void) {
        queue.async {
            do {
                let profile = Profile(raw: try FxAError.unwrap({err in
                    fxa_profile(self.raw, false, err)
                }))
                DispatchQueue.main.async { completionHandler(profile, nil) }
            } catch {
                DispatchQueue.main.async { completionHandler(nil, error) }
            }
        }
    }

    #if BROWSERID_FEATURES
    public func getSyncKeys() throws -> SyncKeys {
        return try queue.sync(execute: {
            return SyncKeys(raw: try FxAError.unwrap({err in
                fxa_get_sync_keys(self.raw, err)
            }))
        })
    }
    #endif

    open func getTokenServerEndpointURL() throws -> URL {
        return try queue.sync(execute: {
            return URL(string: String(freeingFxaString: try FxAError.unwrap({err in
                fxa_get_token_server_endpoint_url(self.raw, err)
            })))!
        })
    }

    open func getConnectionSuccessURL() throws -> URL {
        return try queue.sync(execute: {
            return URL(string: String(freeingFxaString: try FxAError.unwrap({err in
                fxa_get_connection_success_url(self.raw, err)
            })))!
        })
    }

    /// Request a OAuth token by starting a new OAuth flow.
    ///
    /// This function returns a URL string that the caller should open in a webview.
    ///
    /// Once the user has confirmed the authorization grant, they will get redirected to `redirect_url`:
    /// the caller must intercept that redirection, extract the `code` and `state` query parameters and call
    /// `completeOAuthFlow(...)` to complete the flow.
    ///
    /// It is possible also to request keys (e.g. sync keys) during that flow by setting `wants_keys` to true.
    open func beginOAuthFlow(scopes: [String], wantsKeys: Bool, completionHandler: @escaping (URL?, Error?) -> Void) {
        queue.async {
            do {
                let scope = scopes.joined(separator: " ")
                let url = URL(string: String(freeingFxaString: try FxAError.unwrap({err in
                    fxa_begin_oauth_flow(self.raw, scope, wantsKeys, err)
                })))!
                DispatchQueue.main.async { completionHandler(url, nil) }
            } catch {
                DispatchQueue.main.async { completionHandler(nil, error) }
            }
        }
    }

    /// Finish an OAuth flow initiated by `beginOAuthFlow(...)` and returns token/keys.
    ///
    /// This resulting token might not have all the `scopes` the caller have requested (e.g. the user
    /// might have denied some of them): it is the responsibility of the caller to accomodate that.
    open func completeOAuthFlow(code: String, state: String, completionHandler: @escaping (Void, Error?) -> Void) {
        queue.async {
            do {
                try FxAError.unwrap({err in
                    fxa_complete_oauth_flow(self.raw, code, state, err)
                })
                DispatchQueue.main.async { completionHandler((), nil) }
            } catch {
                DispatchQueue.main.async { completionHandler((), error) }
            }
        }
    }

    /// Try to get an OAuth access token.
    ///
    /// Throws `FxAError.Unauthorized` if we couldn't provide an access token
    /// for this scope. The caller should then start the OAuth Flow again with
    /// the desired scope.
    open func getAccessToken(scope: String, completionHandler: @escaping (AccessTokenInfo?, Error?) -> Void) {
        queue.async {
            do {
                let tokenInfo = AccessTokenInfo(raw: try FxAError.unwrap({err in
                    fxa_get_access_token(self.raw, scope, err)
                }))
                DispatchQueue.main.async { completionHandler(tokenInfo, nil) }
            } catch {
                DispatchQueue.main.async { completionHandler(nil, error) }
            }
        }
    }

    #if BROWSERID_FEATURES
    public func generateAssertion(audience: String) throws -> String {
        return try queue.sync(execute: {
            return String(freeingFxaString: try FxAError.unwrap({err in
                fxa_assertion_new(raw, audience, err)
            }))
        })
    }
    #endif
}

/**
 This function needs to be static as callbacks passed into Rust from Swift cannot contain state. Therefore the observers are static, as is
 the function that we pass into Rust to receive the callback.
 */
private func persistCallbackFunction(json: UnsafePointer<CChar>) {
    let json = String(cString: json)
    if let cb = FirefoxAccount.persistCallback {
        DispatchQueue.global(qos: .background).async {
            cb.persist(json: json)
        }
    }
}

open class AccessTokenInfo: RustStructPointer<AccessTokenInfoC> {
    open var scope: String {
        get {
            return String(cString: raw.pointee.scope)
        }
    }

    open var token: String {
        get {
            return String(cString: raw.pointee.token)
        }
    }

    open var key: String? {
        get {
            guard let pointer = raw.pointee.key else {
                return nil
            }
            return String(cString: pointer)
        }
    }

    open var expiresAt: Date {
        get {
            return Date.init(timeIntervalSince1970: Double(raw.pointee.expires_at))
        }
    }

    override func cleanup(pointer: UnsafeMutablePointer<AccessTokenInfoC>) {
        queue.sync {
            fxa_oauth_info_free(self.raw)
        }
    }
}

public struct Avatar {
    let url: String
    let isDefault: Bool
}

open class Profile: RustStructPointer<ProfileC> {
    open var uid: String {
        get {
            return String(cString: raw.pointee.uid)
        }
    }

    open var email: String {
        get {
            return String(cString: raw.pointee.email)
        }
    }

    open var avatar: Avatar? {
        get {
            guard let pointer = raw.pointee.avatar else {
                return nil
            }
            return Avatar(url: String(cString: pointer), isDefault: raw.pointee.avatar_default == 0x01)
        }
    }

    open var displayName: String? {
        get {
            guard let pointer = raw.pointee.display_name else {
                return nil
            }
            return String(cString: pointer)
        }
    }

    override func cleanup(pointer: UnsafeMutablePointer<ProfileC>) {
        queue.sync {
            fxa_profile_free(raw)
        }
    }
}

open class SyncKeys: RustStructPointer<SyncKeysC> {
    open var syncKey: String {
        get {
            return String(cString: raw.pointee.sync_key)
        }
    }

    open var xcs: String {
        get {
            return String(cString: raw.pointee.xcs)
        }
    }

    override func cleanup(pointer: UnsafeMutablePointer<SyncKeysC>) {
        queue.sync {
            fxa_sync_keys_free(raw)
        }
    }
}

