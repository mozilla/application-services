/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation
import UIKit

// We use a serial queue to protect access to the rust object.
let queue = DispatchQueue(label: "com.fxaclient")

public class FxAConfig: MovableRustOpaquePointer {
    open class func release(completionHandler: @escaping (FxAConfig?, Error?) -> Void) {
        queue.async {
            do {
                let config = FxAConfig(raw: try FxAError.unwrap({err in
                    fxa_get_release_config(err)
                }))
                DispatchQueue.main.async { completionHandler(config, nil) }
            } catch {
                DispatchQueue.main.async { completionHandler(nil, error) }
            }
        }
    }

    open class func custom(content_base: String, completionHandler: @escaping (FxAConfig?, Error?) -> Void) {
        queue.async {
            do {
                let config = FxAConfig(raw: try FxAError.unwrap({err in
                    fxa_get_custom_config(content_base, err)
                }))
                DispatchQueue.main.async { completionHandler(config, nil) }
            } catch {
                DispatchQueue.main.async { completionHandler(nil, error) }
            }
        }
    }

    override func cleanup(pointer: OpaquePointer) {
        queue.sync {
            fxa_config_free(pointer)
        }
    }
}

public class FirefoxAccount: RustOpaquePointer {
    #if BROWSERID_FEATURES
    open class func from(config: FxAConfig, clientId: String, redirectUri: String, webChannelResponse: String) throws -> FirefoxAccount {
        return try queue.sync(execute: {
            let pointer = try FxAError.unwrap({err in
                fxa_from_credentials(try config.movePointer(), clientId, redirectUri, webChannelResponse, err)
            })
            return FirefoxAccount(raw: pointer)
        })
    }
    #endif

    open class func fromJSON(state: String) throws -> FirefoxAccount {
        return try queue.sync(execute: {
            let pointer = try FxAError.unwrap({ err in fxa_from_json(state, err) })
            return FirefoxAccount(raw: pointer)
        })
    }

    public convenience init(config: FxAConfig, clientId: String, redirectUri: String) throws {
        let pointer = try queue.sync(execute: {
            return try FxAError.unwrap({err in
                fxa_new(try config.movePointer(), clientId, redirectUri, err)
            })
        })
        self.init(raw: pointer)
    }

    override func cleanup(pointer: OpaquePointer) {
        queue.sync(execute: {
            fxa_free(pointer)
        })
    }

    public func toJSON() throws -> String {
        return try queue.sync(execute: {
            return String(freeingFxaString: try FxAError.unwrap({err in
                fxa_to_json(self.raw, err)
            }))
        })
    }

    public func getProfile(completionHandler: @escaping (Profile?, Error?) -> Void) {
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

    public func getTokenServerEndpointURL() throws -> URL {
        return try queue.sync(execute: {
            return URL(string: String(freeingFxaString: try FxAError.unwrap({err in
                fxa_get_token_server_endpoint_url(self.raw, err)
            })))!
        })
    }

    // Scopes is space separated for each scope.
    public func beginOAuthFlow(scopes: [String], wantsKeys: Bool, completionHandler: @escaping (URL?, Error?) -> Void) {
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

    public func completeOAuthFlow(code: String, state: String, completionHandler: @escaping (OAuthInfo?, Error?) -> Void) {
        queue.async {
            do {
                let oauthInfo = OAuthInfo(raw: try FxAError.unwrap({err in
                    fxa_complete_oauth_flow(self.raw, code, state, err)
                }))
                DispatchQueue.main.async { completionHandler(oauthInfo, nil) }
            } catch {
                DispatchQueue.main.async { completionHandler(nil, error) }
            }
        }
    }


    public func getOAuthToken(scopes: [String], completionHandler: @escaping (OAuthInfo?, Error?) -> Void) {
        queue.async {
            do {
                let scope = scopes.joined(separator: " ")
                let info = try FxAError.tryUnwrap({err in
                    fxa_get_oauth_token(self.raw, scope, err)
                })
                guard let ptr: UnsafeMutablePointer<OAuthInfoC> = info else {
                    DispatchQueue.main.async { completionHandler(nil, nil) }
                    return
                }
                let oauthInfo = OAuthInfo(raw: ptr)
                    DispatchQueue.main.async { completionHandler(oauthInfo, nil) }
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

public class OAuthInfo: RustStructPointer<OAuthInfoC> {
    public var scopes: [String] {
        get {
            return String(cString: raw.pointee.scope).components(separatedBy: " ")
        }
    }

    public var accessToken: String {
        get {
            return String(cString: raw.pointee.access_token)
        }
    }

    public var keys: String? {
        get {
            guard let pointer = raw.pointee.keys else {
                return nil
            }
            return String(cString: pointer)
        }
    }

    override func cleanup(pointer: UnsafeMutablePointer<OAuthInfoC>) {
        queue.sync {
            fxa_oauth_info_free(self.raw)
        }
    }
}

public class Profile: RustStructPointer<ProfileC> {
    public var uid: String {
        get {
            return String(cString: raw.pointee.uid)
        }
    }

    public var email: String {
        get {
            return String(cString: raw.pointee.email)
        }
    }

    public var avatar: String {
        get {
            return String(cString: raw.pointee.avatar)
        }
    }

    public var displayName: String? {
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

public class SyncKeys: RustStructPointer<SyncKeysC> {
    public var syncKey: String {
        get {
            return String(cString: raw.pointee.sync_key)
        }
    }

    public var xcs: String {
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

