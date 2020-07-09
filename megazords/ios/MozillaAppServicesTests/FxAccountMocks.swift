/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation
@testable import MozillaAppServices

// Arrays are not thread-safe in Swift.
let queue = DispatchQueue(label: "InvocationsArrayQueue")

class MockFxAccount: FxAccount {
    var invocations: [MethodInvocation] = []
    enum MethodInvocation {
        case checkAuthorizationStatus
        case ensureCapabilities
        case getProfile
        case registerPersistCallback
        case clearAccessTokenCache
        case getAccessToken
        case initializeDevice
        case getDevices
    }

    init() {
        super.init(raw: 0)
    }

    required convenience init(fromJsonState _: String) throws {
        fatalError("init(fromJsonState:) has not been implemented")
    }

    required convenience init(config _: FxAConfig) throws {
        fatalError("init(config:) has not been implemented")
    }

    override func isInMigrationState() -> Bool {
        return false
    }

    override func initializeDevice(name _: String, deviceType _: DeviceType, supportedCapabilities _: [DeviceCapability]) throws {
        queue.sync { invocations.append(.initializeDevice) }
    }

    override func getDevices(ignoreCache _: Bool) throws -> [Device] {
        queue.sync { invocations.append(.getDevices) }
        return []
    }

    override func registerPersistCallback(_: PersistCallback) {
        queue.sync { invocations.append(.registerPersistCallback) }
    }

    override func ensureCapabilities(supportedCapabilities _: [DeviceCapability]) throws {
        queue.sync { invocations.append(.ensureCapabilities) }
    }

    override func checkAuthorizationStatus() throws -> IntrospectInfo {
        queue.sync { invocations.append(.checkAuthorizationStatus) }
        return IntrospectInfo(active: true)
    }

    override func clearAccessTokenCache() throws {
        queue.sync { invocations.append(.clearAccessTokenCache) }
    }

    override func getAccessToken(scope _: String, ttl _: UInt64? = nil) throws -> AccessTokenInfo {
        queue.sync { invocations.append(.getAccessToken) }
        return AccessTokenInfo(scope: "profile", token: "toktok")
    }

    override func getProfile(ignoreCache _: Bool) throws -> Profile {
        queue.sync { invocations.append(.getProfile) }
        return Profile(uid: "uid", email: "foo@bar.bobo")
    }

    override func beginOAuthFlow(scopes _: [String], entrypoint _: String) throws -> URL {
        return URL(string: "https://foo.bar/oauth?state=bobo")!
    }
}

class MockFxAccountManager: FxAccountManager {
    var storedAccount: FxAccount?

    override func createAccount() -> FxAccount {
        return MockFxAccount()
    }

    override func makeDeviceConstellation(account _: FxAccount) -> DeviceConstellation {
        return MockDeviceConstellation(account: account)
    }

    override func tryRestoreAccount() -> FxAccount? {
        return storedAccount
    }
}

class MockDeviceConstellation: DeviceConstellation {
    var invocations: [MethodInvocation] = []
    enum MethodInvocation {
        case ensureCapabilities
        case initDevice
        case refreshState
    }

    override init(account: FxAccount?) {
        super.init(account: account ?? MockFxAccount())
    }

    override func initDevice(name: String, type: DeviceType, capabilities: [DeviceCapability]) {
        queue.sync { invocations.append(.initDevice) }
        super.initDevice(name: name, type: type, capabilities: capabilities)
    }

    override func ensureCapabilities(capabilities: [DeviceCapability]) {
        queue.sync { invocations.append(.ensureCapabilities) }
        super.ensureCapabilities(capabilities: capabilities)
    }

    override func refreshState() {
        queue.sync { invocations.append(.refreshState) }
        super.refreshState()
    }
}

func mockFxAManager() -> MockFxAccountManager {
    return MockFxAccountManager(
        config: FxAConfig(server: .release, clientId: "clientid", redirectUri: "redirect"),
        deviceConfig: DeviceConfig(name: "foo", type: .mobile, capabilities: [])
    )
}
