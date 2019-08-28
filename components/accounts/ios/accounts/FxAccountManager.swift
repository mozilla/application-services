/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation
import os.log
import UIKit

private let queue = DispatchQueue(label: "com.mozilla.accounts")

// TODO: are these names idiomatic?
extension Notification.Name {
    // Account was successfully authenticated.
    static let onAuthenticated = Notification.Name("onAuthenticated")
    // Account's profile is now available.
    static let onProfileUpdated = Notification.Name("onProfileUpdated")
    // Account needs to be re-authenticated (e.g. due to a password change).
    static let onAuthenticationProblems = Notification.Name("onAuthenticationProblems")
    // Account just got logged out.
    static let onLoggedOut = Notification.Name("onLoggedOut")
    // Device list updated.
    static let onDevicesUpdate = Notification.Name("onDevicesUpdate")
    // Incoming device event (e.g. Send Tab).
    static let onDeviceEvents = Notification.Name("onDeviceEvents")
}

public enum AccountState {
    case Start
    case NotAuthenticated
    case Authenticated
    case AuthenticationProblem

    internal init(msg: MsgTypes_AccountState) {
        switch msg.state {
        case .start: self = .Start
        case .notAuthenticated: self = .NotAuthenticated
        case .authenticated: self = .Authenticated
        case .authenticationProblem: self = .AuthenticationProblem
        }
    }
}

open class FxAccountManager {
    private let raw: UInt64
    private let serverConfig: FxAConfig
    private let deviceConfig: DeviceConfig
    private let accountStorage: AccountStorage = UnsafeUserDefaultStorage()
    public let deviceConstellation: FxADeviceConstellation = FxADeviceConstellation()

    private init(raw: UInt64, serverConfig: FxAConfig, deviceConfig: DeviceConfig) {
        self.raw = raw
        self.serverConfig = serverConfig
        self.deviceConfig = deviceConfig
        deviceConstellation.parent = self
    }

    // TODO:
    public convenience init(serverConfig: FxAConfig, deviceConfig: DeviceConfig) throws {
        // TODO: in a helper!
        var capabilities = MsgTypes_Capabilities()
        deviceConfig.capabilities.forEach { capabilities.capability.append($0.toMessage()) }
        let capabilitiesData = try! capabilities.serializedData()
        let size = Int32(capabilitiesData.count)

        let pointer = try queue.sync {
            try capabilitiesData.withUnsafeBytes { (bytes: UnsafePointer<UInt8>) in
                try FirefoxAccountError.unwrap { err in
                    fxa_mgr_new(
                        serverConfig.contentUrl,
                        serverConfig.clientId,
                        serverConfig.redirectUri,
                        deviceConfig.name,
                        Int32(deviceConfig.type.toMessage().rawValue),
                        bytes,
                        size,
                        err
                    )
                }
            }
        }
        self.init(raw: pointer, serverConfig: serverConfig, deviceConfig: deviceConfig)
    }

    deinit {
        if self.raw != 0 {
            queue.sync {
                try! FirefoxAccountError.unwrap { err in
                    // Is `try!` the right thing to do? We should only hit an error here
                    // for panics and handle misuse, both inidicate bugs in our code
                    // (the first in the rust code, the 2nd in this swift wrapper).
                    fxa_mgr_free(self.raw, err)
                }
            }
        }
    }

    public var accountState: AccountState {
        return queue.sync {
            do {
                let stateBuffer = try FirefoxAccountError.unwrap { err in
                    fxa_mgr_account_state(self.raw, err)
                }
                let msg = try! MsgTypes_AccountState(serializedData: Data(rustBuffer: stateBuffer))
                fxa_mgr_bytebuffer_free(stateBuffer)
                return AccountState(msg: msg)
            } catch {
                os_log("Error while calling fxa_mgr_account_state")
                return .Start
            }
        }
    }

    public func initialize() {
        queue.async { // The do-catch inside queue.async pattern may be factorized?
            do {
                let jsonState = self.accountStorage.read()
                try FirefoxAccountError.unwrap { err in
                    fxa_mgr_init(self.raw, jsonState, err)
                }
                self.persistAccount()
                switch self.accountState {
                case .Authenticated: self.postAuthenticated()
                case .AuthenticationProblem: NotificationCenter.default.post(name: .onAuthenticationProblems, object: nil)
                default: do {} // Nothing
                }
            } catch {
                os_log("Error while calling fxa_mgr_init")
            }
        }
    }

    public func beginOAuthFlow(completionHandler: @escaping (URL?, Error?) -> Void) {
        queue.async {
            do {
                let url = URL(string: String(freeingAccountsString: try FirefoxAccountError.unwrap { err in
                    fxa_mgr_begin_oauth_flow(self.raw, err)
                }))!
                DispatchQueue.main.async { completionHandler(url, nil) }
            } catch {
                DispatchQueue.main.async { completionHandler(nil, error) }
            }
        }
    }

    public func beginPairingFlow(pairingUrl: String, completionHandler: @escaping (URL?, Error?) -> Void) {
        queue.async {
            do {
                let url = URL(string: String(freeingAccountsString: try FirefoxAccountError.unwrap { err in
                    fxa_mgr_begin_pairing_flow(self.raw, pairingUrl, err)
                }))!
                DispatchQueue.main.async { completionHandler(url, nil) }
            } catch {
                DispatchQueue.main.async { completionHandler(nil, error) }
            }
        }
    }

    public func finishAuthentication(code: String, state: String) {
        queue.async {
            // TODO: if that call has no errors we should just not take `err`.
            try! FirefoxAccountError.unwrap { err in
                fxa_mgr_finish_authentication_flow(self.raw, code, state, err)
            }
            self.persistAccount()
            switch self.accountState {
            case .Authenticated: self.postAuthenticated()
            case .AuthenticationProblem: NotificationCenter.default.post(name: .onAuthenticationProblems, object: nil)
            default: do {} // Nothing
            }
        }
    }

    internal func postAuthenticated() {
        NotificationCenter.default.post(name: .onAuthenticated, object: nil)
        updateProfile() // This will subsquently trigger a .onProfileUpdated notification.
    }

    public func updateProfile() {
        queue.async {
            let oldAccountState = self.accountState
            let maybeProfile = try! FxAccountManager.unwrapProfile {
                try FirefoxAccountError.unwrap { err in
                    fxa_mgr_update_profile(self.raw, err)
                }
            }
            self.persistAccount()
            if self.accountState == .AuthenticationProblem, oldAccountState == .Authenticated {
                NotificationCenter.default.post(name: .onAuthenticationProblems, object: nil)
            }
            if let profile = maybeProfile {
                NotificationCenter.default.post(name: .onProfileUpdated, object: nil, userInfo: ["profile": profile])
            }
        }
    }

    public var profile: Profile? {
        return queue.sync {
            try! FxAccountManager.unwrapProfile {
                try FirefoxAccountError.unwrap { err in
                    fxa_mgr_get_profile(self.raw, err)
                }
            }
        }
    }

    internal static func unwrapProfile(_ callback: () throws -> FxAManagerRustBuffer) throws -> Profile? {
        let profileBuffer = try callback()
        if profileBuffer.data == nil {
            return nil
        }
        let msg = try MsgTypes_Profile(serializedData: Data(rustBuffer: profileBuffer))
        fxa_mgr_bytebuffer_free(profileBuffer)
        return Profile(msg: msg)
    }

    public func onAuthenticationError() {
        queue.async {
            try! FirefoxAccountError.unwrap { err in
                fxa_mgr_on_authentication_error(self.raw, err)
            }
            if self.accountState == .Authenticated {
                NotificationCenter.default.post(name: .onAuthenticated, object: nil)
            }
        }
    }

    public func logout() {
        queue.async {
            try! FirefoxAccountError.unwrap { err in
                fxa_mgr_logout(self.raw, err)
            }
            NotificationCenter.default.post(name: .onLoggedOut, object: nil)
        }
    }

    public class FxADeviceConstellation {
        weak var parent: FxAccountManager!

        public var state: ConstellationState {
            return queue.sync {
                try! FxADeviceConstellation.unwrapDevices {
                    try FirefoxAccountError.unwrap { err in
                        fxa_mgr_get_devices(self.parent.raw, err)
                    }
                }
            }
        }

        public func refreshDevices() {
            queue.async {
                let oldAccountState = self.parent.accountState
                let devices = try! FxADeviceConstellation.unwrapDevices {
                    try FirefoxAccountError.unwrap { err in
                        fxa_mgr_update_devices(self.parent.raw, err)
                    }
                }
                self.parent.persistAccount()
                if self.parent.accountState == .AuthenticationProblem, oldAccountState == .Authenticated {
                    NotificationCenter.default.post(name: .onAuthenticationProblems, object: nil)
                }
                NotificationCenter.default.post(name: .onDevicesUpdate, object: nil, userInfo: ["devices": devices])
            }
        }

        internal static func unwrapDevices(_ callback: () throws -> FxAManagerRustBuffer) throws -> ConstellationState {
            let devicesBuffer = try callback()
            let msg = try MsgTypes_DeviceConstellation(serializedData: Data(rustBuffer: devicesBuffer))
            fxa_mgr_bytebuffer_free(devicesBuffer)
            return ConstellationState(msg: msg)
        }

        public func processRawEvent(payload: String) {
            queue.async {
                let events = try! FxADeviceConstellation.unwrapDeviceEvents {
                    try FirefoxAccountError.unwrap { err in
                        fxa_mgr_handle_push_message(self.parent.raw, payload, err)
                    }
                }
                self.parent.persistAccount()
                if !events.isEmpty {
                    NotificationCenter.default.post(name: .onDeviceEvents, object: nil, userInfo: ["events": events])
                }
            }
        }

        public func pollForEvents() {
            queue.async {
                let events = try! FxADeviceConstellation.unwrapDeviceEvents {
                    try FirefoxAccountError.unwrap { err in
                        fxa_mgr_poll_device_commands(self.parent.raw, err)
                    }
                }
                self.parent.persistAccount()
                if !events.isEmpty {
                    NotificationCenter.default.post(name: .onDeviceEvents, object: nil, userInfo: ["events": events])
                }
            }
        }

        internal static func unwrapDeviceEvents(_ callback: () throws -> FxAManagerRustBuffer) throws -> [DeviceEvent] {
            let events = try callback()
            let msg = try MsgTypes_AccountEvents(serializedData: Data(rustBuffer: events))
            fxa_mgr_bytebuffer_free(events)
            return msg.events.map { DeviceEvent(msg: $0) }
        }

        public func setDeviceName(name: String) {
            queue.async {
                let oldAccountState = self.parent.accountState
                // TODO: try! is actually wrong here, as this can error out!
                try! FirefoxAccountError.unwrap { err in
                    fxa_mgr_set_device_name(self.parent.raw, name, err)
                }
                if self.parent.accountState == .AuthenticationProblem, oldAccountState == .Authenticated {
                    NotificationCenter.default.post(name: .onAuthenticationProblems, object: nil)
                }
            }
        }

        public func setDevicePushSubscription(sub: Device.PushSubscription) {
            queue.async {
                let oldAccountState = self.parent.accountState
                // TODO: try! is actually wrong here, as this can error out!
                try! FirefoxAccountError.unwrap { err in
                    fxa_mgr_set_push_subscription(self.parent.raw, sub.endpoint, sub.publicKey, sub.authKey, err)
                }
                if self.parent.accountState == .AuthenticationProblem, oldAccountState == .Authenticated {
                    NotificationCenter.default.post(name: .onAuthenticationProblems, object: nil)
                }
            }
        }

        public func sendEventToDevice(targetDeviceId: String, event: DeviceEventOutgoing) {
            queue.async {
                let oldAccountState = self.parent.accountState
                switch event {
                case let .SendTab(title, url): do {
                    // TODO: checkout try!s all throughout the file actually
                    try! FirefoxAccountError.unwrap { err in
                        fxa_mgr_send_tab(self.parent.raw, targetDeviceId, title, url, err)
                    }
                }
                }
                if self.parent.accountState == .AuthenticationProblem, oldAccountState == .Authenticated {
                    NotificationCenter.default.post(name: .onAuthenticationProblems, object: nil)
                }
            }
        }
    }

    internal func persistAccount() {
        do {
            let accountState = String(freeingAccountsString: try FirefoxAccountError.unwrap { err in
                fxa_mgr_export_persisted_state(self.raw, err)
            })
            accountStorage.write(accountState: accountState)
        } catch {
            os_log("Unable to persist account state")
        }
    }
}
