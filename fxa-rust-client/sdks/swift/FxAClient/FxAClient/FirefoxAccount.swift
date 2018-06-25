/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation
import UIKit

public class FxAConfig: MovableRustOpaquePointer {
    open class func release() throws -> FxAConfig {
        return FxAConfig(raw: try FxAError.unwrap({err in
            fxa_get_release_config(err)
        }))
    }

    open class func custom(content_base: String) throws -> FxAConfig {
        return FxAConfig(raw: try FxAError.unwrap({err in
            fxa_get_custom_config(content_base, err)
        }))
    }

    override func cleanup(pointer: OpaquePointer) {
        fxa_config_free(pointer)
    }
}

public class FirefoxAccount: RustOpaquePointer {
    // webChannelResponse is a string for now, but will probably be a JSON
    // object in the future.
    open class func from(config: FxAConfig, clientId: String, webChannelResponse: String) throws -> FirefoxAccount {
        let pointer = try FxAError.unwrap({err in
            fxa_from_credentials(try config.movePointer(), clientId, webChannelResponse, err)
        })
        return FirefoxAccount(raw: pointer)
    }

    open class func fromJSON(state: String) throws -> FirefoxAccount {
        let pointer = try FxAError.unwrap({ err in fxa_from_json(state, err) })
        return FirefoxAccount(raw: pointer)
    }

    public convenience init(config: FxAConfig, clientId: String) throws {
        let pointer = try FxAError.unwrap({err in
            fxa_new(try config.movePointer(), clientId, err)
        })
        self.init(raw: pointer)
    }

    override func cleanup(pointer: OpaquePointer) {
        fxa_free(pointer)
    }

    public func toJSON() throws -> String {
        return String(freeingFxaString: try FxAError.unwrap({err in
            fxa_to_json(self.raw, err)
        }))
    }

    public func getProfile() throws -> Profile {
        return Profile(raw: try FxAError.unwrap({err in
            fxa_profile(self.raw, false, err)
        }))
    }

    public func getSyncKeys() throws -> SyncKeys {
        return SyncKeys(raw: try FxAError.unwrap({err in
            fxa_get_sync_keys(self.raw, err)
        }))
    }

    public func getTokenServerEndpointURL() throws -> URL {
        return URL(string: String(freeingFxaString: try FxAError.unwrap({err in
            fxa_get_token_server_endpoint_url(self.raw, err)
        })))!
    }

    // Scopes is space separated for each scope.
    public func beginOAuthFlow(redirectURI: String, scopes: [String], wantsKeys: Bool) throws -> URL {
        let scope = scopes.joined(separator: " ");
        return URL(string: String(freeingFxaString: try FxAError.unwrap({err in
            fxa_begin_oauth_flow(raw, redirectURI, scope, wantsKeys, err)
        })))!
    }

    public func completeOAuthFlow(code: String, state: String) throws -> OAuthInfo {
        return OAuthInfo(raw: try FxAError.unwrap({err in
             fxa_complete_oauth_flow(self.raw, code, state, err)
        }))
    }

    public func getOAuthToken(scopes: [String]) throws -> OAuthInfo? {
        let scope = scopes.joined(separator: " ")
        let info = try FxAError.tryUnwrap({err in
            fxa_get_oauth_token(self.raw, scope, err)
        })
        guard let ptr: UnsafeMutablePointer<OAuthInfoC> = info else {
            return nil
        }
        return OAuthInfo(raw: ptr)
    }

    public func generateAssertion(audience: String) throws -> String {
        return String(freeingFxaString: try FxAError.unwrap({err in
            fxa_assertion_new(raw, audience, err)
        }))
    }
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
        fxa_oauth_info_free(self.raw)
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
        fxa_profile_free(raw)
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
        fxa_sync_keys_free(raw)
    }
}

