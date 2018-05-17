/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation
import UIKit

open class FxAConfig: RustObject {
    var raw: OpaquePointer
    var wasMoved = false

    open class func release() -> FxAConfig {
        return FxAConfig(raw: fxa_get_release_config())
    }

    open class func custom(content_base: String) -> FxAConfig {
        return FxAConfig(raw: fxa_get_custom_config(content_base))
    }

    required public init(raw: OpaquePointer) {
        self.raw = raw
    }

    func intoRaw() -> OpaquePointer {
        self.wasMoved = true
        return self.raw
    }

    deinit {
        if !wasMoved {
            fxa_config_free(raw)
        }
    }
}

open class FirefoxAccount: RustObject {
    var raw: OpaquePointer

    // webChannelResponse is a string for now, but will probably be a JSON
    // object in the future.
    open class func from(config: FxAConfig, clientId: String, webChannelResponse: String) -> FirefoxAccount {
        return FirefoxAccount(raw: fxa_from_credentials(config.intoRaw(), clientId, webChannelResponse))
    }

    open class func fromJSON(state: String) -> FirefoxAccount {
        return FirefoxAccount(raw: fxa_from_json(state))
    }

    public init(config: FxAConfig, clientId: String) {
        self.raw = fxa_new(config.intoRaw(), clientId)
    }

    required public init(raw: OpaquePointer) {
        self.raw = raw
    }

    public func toJSON() -> Optional<String> {
        guard let pointer = fxa_to_json(raw) else {
            return nil
        }
        return copy_and_free_str(pointer)
    }

    func intoRaw() -> OpaquePointer {
        return self.raw
    }

    deinit {
        fxa_free(raw)
    }

    public var profile: Optional<Profile> {
        get {
            guard let pointer = fxa_profile(raw) else {
                return nil
            }
            return Profile(raw: pointer)
        }
    }

    public var syncKeys: Optional<SyncKeys> {
        get {
            guard let pointer = fxa_get_sync_keys(raw) else {
                return nil
            }
            return SyncKeys(raw: pointer)
        }
    }

    public var tokenServerEndpointURL: Optional<URL> {
        get {
            guard let pointer = fxa_get_token_server_endpoint_url(raw) else {
                return nil
            }
            return URL(string: copy_and_free_str(pointer))
        }
    }

    // Scopes is space separated for each scope.
    public func beginOAuthFlow(redirectURI: String, scopes: [String], wantsKeys: Bool) -> Optional<URL> {
        let scope = scopes.joined(separator: " ");
        guard let pointer = fxa_begin_oauth_flow(raw, redirectURI, scope, wantsKeys) else {
            return nil
        }
        return URL(string: String(cString: pointer))
    }

    public func completeOAuthFlow(code: String, state: String) -> Optional<OAuthInfo> {
        guard let pointer = fxa_complete_oauth_flow(raw, code, state) else {
            return nil
        }
        return OAuthInfo(raw: pointer)
    }

    public func getOAuthToken(scopes: [String]) -> Optional<OAuthInfo> {
        let scope = scopes.joined(separator: " ");
        guard let pointer = fxa_get_oauth_token(raw, scope) else {
            return nil
        }
        return OAuthInfo(raw: pointer)
    }

    public func generateAssertion(audience: String) -> Optional<String> {
        guard let pointer = fxa_assertion_new(raw, audience) else {
            return nil
        }
        return copy_and_free_str(pointer)
    }
}

open class OAuthInfo {
    var raw: UnsafeMutablePointer<OAuthInfoC>

    public init(raw: UnsafeMutablePointer<OAuthInfoC>) {
        self.raw = raw
    }

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

    public var keysJWE: Optional<String> {
        get {
            if (raw.pointee.keys_jwe == nil) {
                return nil
            }
            return String(cString: raw.pointee.keys_jwe)
        }
    }

    deinit {
        fxa_oauth_info_free(raw)
    }
}

open class Profile {
    var raw: UnsafeMutablePointer<ProfileC>

    public init(raw: UnsafeMutablePointer<ProfileC>) {
        self.raw = raw
    }

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

    deinit {
        fxa_profile_free(raw)
    }
}

open class SyncKeys {
    var raw: UnsafeMutablePointer<SyncKeysC>

    public init(raw: UnsafeMutablePointer<SyncKeysC>) {
        self.raw = raw
    }

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

    deinit {
        fxa_sync_keys_free(raw)
    }
}

func copy_and_free_str(_ pointer: UnsafeMutablePointer<Int8>) -> String {
    let copy = String(cString: pointer)
    fxa_free_str(pointer)
    return copy
}
