/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation
import UIKit

public class FxAConfig: MovableRustOpaquePointer {
    open class func release(completionHandler: @escaping (FxAConfig?, Error?) -> Void) {
        let concurrentQueue = DispatchQueue(label: "com.fxaclient.release", attributes: .concurrent)
        concurrentQueue.async {
            do {
                let pointer = try fxa_get_release_config().pointee.unwrap()
                completionHandler(FxAConfig(raw: pointer), nil)
            } catch {
                completionHandler(nil, error)
            }
        }
    }

    open class func custom(content_base: String, completionHandler: @escaping (FxAConfig?, Error?) -> Void) {
        let concurrentQueue = DispatchQueue(label: "com.fxaclient.custom", attributes: .concurrent)
        concurrentQueue.async {
            do {
                let pointer = try fxa_get_custom_config(content_base).pointee.unwrap()
                completionHandler(FxAConfig(raw: pointer), nil)
            } catch {
                completionHandler(nil, error)
            }
        }
    }

    override func cleanup(pointer: OpaquePointer) {
        fxa_config_free(pointer)
    }
}

public class FirefoxAccount: RustOpaquePointer {
    // webChannelResponse is a string for now, but will probably be a JSON
    // object in the future.
    open class func from(config: FxAConfig, clientId: String, webChannelResponse: String) throws -> FirefoxAccount {
        let pointer = try fxa_from_credentials(config.validPointer(), clientId, webChannelResponse).pointee.unwrap()
        config.raw = nil
        return FirefoxAccount(raw: pointer)
    }

    open class func fromJSON(state: String) throws -> FirefoxAccount {
        let pointer = try fxa_from_json(state).pointee.unwrap()
        return FirefoxAccount(raw: pointer)
    }

    public convenience init(config: FxAConfig, clientId: String) throws {
        let pointer = try fxa_new(config.validPointer(), clientId).pointee.unwrap()
        config.raw = nil
        self.init(raw: pointer)
    }

    override func cleanup(pointer: OpaquePointer) {
        fxa_free(pointer)
    }

    public func toJSON() throws -> String {
        return copy_and_free_str(try fxa_to_json(self.raw).pointee.unwrap())
    }

    public func getProfile(completionHandler: @escaping (Profile?, Error?) -> Void) {
        let concurrentQueue = DispatchQueue(label: "com.fxaclient.release", attributes: .concurrent)
        concurrentQueue.async {
            do {
                guard let oauth_token = try self.getOAuthToken(scopes: ["profile"]) else {
                    throw FxAError.Unauthorized(message: "No suitable cached OAuth token found for this operation.")
                }
                let profile = Profile(raw: try fxa_profile(self.raw, oauth_token.accessToken, false).pointee.unwrap())
                completionHandler(profile, nil)
            } catch {
                completionHandler(nil, error)
            }
        }
    }

    public func getSyncKeys() throws -> SyncKeys {
        return SyncKeys(raw: try fxa_get_sync_keys(self.raw).pointee.unwrap())
    }

    public func getTokenServerEndpointURL() throws -> URL {
        return URL(string: copy_and_free_str(try fxa_get_token_server_endpoint_url(self.raw).pointee.unwrap()))!
    }

    // Scopes is space separated for each scope.
    public func beginOAuthFlow(redirectURI: String, scopes: [String], wantsKeys: Bool, completionHandler: @escaping (URL?, Error?) -> Void) {
        let concurrentQueue = DispatchQueue(label: "com.fxaclient.beginoauthflow", attributes: .concurrent)
        concurrentQueue.async {
            let scope = scopes.joined(separator: " ");
            do {
                let urlString: String = copy_and_free_str(try fxa_begin_oauth_flow(self.raw, redirectURI, scope, wantsKeys).pointee.unwrap())
                completionHandler(URL(string: urlString)!, nil)
            } catch {
                completionHandler(nil, error)
            }
        }
    }

    public func completeOAuthFlow(code: String, state: String, completionHandler: @escaping (OAuthInfo?, Error?) -> Void) {
        let concurrentQueue = DispatchQueue(label: "com.fxaclient.beginoauthflow", attributes: .concurrent)
        concurrentQueue.async {
            do {
                let result = OAuthInfo(raw: try fxa_complete_oauth_flow(self.raw, code, state).pointee.unwrap())
                completionHandler(result, nil)
            } catch {
                completionHandler(nil, error)
            }
        }
    }

    public func getOAuthToken(scopes: [String]) throws -> OAuthInfo? {
        let scope = scopes.joined(separator: " ")
        guard let ptr: UnsafeMutablePointer<OAuthInfoC> = try fxa_get_oauth_token(raw, scope).pointee.tryUnwrap() else {
            return nil
        }
        return OAuthInfo(raw: ptr)
    }

    public func generateAssertion(audience: String) throws -> String {
        return copy_and_free_str(try fxa_assertion_new(raw, audience).pointee.unwrap())
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

func copy_and_free_str(_ pointer: UnsafeMutablePointer<Int8>) -> String {
    let copy = String(cString: pointer)
    fxa_str_free(pointer)
    return copy
}
