/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation

public struct Device {
    public let id: String
    public let displayName: String
    public let deviceType: DeviceType
    public let isCurrentDevice: Bool
    public let lastAccessTime: UInt64?
    public let capabilities: [DeviceCapability]
    public let subscriptionExpired: Bool
    public let subscription: DevicePushSubscription?

    internal static func fromCollectionMsg(msg: MsgTypes_Devices) -> [Device] {
        msg.devices.map { Device(msg: $0) }
    }

    internal init(msg: MsgTypes_Device) {
        id = msg.id
        displayName = msg.displayName
        deviceType = DeviceType.fromMsg(msg: msg.type)
        isCurrentDevice = msg.isCurrentDevice
        lastAccessTime = msg.hasLastAccessTime ? msg.lastAccessTime : nil
        capabilities = msg.capabilities.map { DeviceCapability.fromMsg(msg: $0) }
        subscriptionExpired = msg.pushEndpointExpired
        subscription = msg.hasPushSubscription ?
            DevicePushSubscription(msg: msg.pushSubscription) :
            nil
    }
}

public enum DeviceType {
    case desktop
    case mobile
    case tablet
    case tv
    case vr
    case unknown

    internal static func fromMsg(msg: MsgTypes_Device.TypeEnum) -> DeviceType {
        switch msg {
        case .desktop: return .desktop
        case .mobile: return .mobile
        case .tablet: return .tablet
        case .tv: return .tv
        case .vr: return .vr
        case .unknown: return .unknown
        }
    }

    internal func toMsg() -> MsgTypes_Device.TypeEnum {
        switch self {
        case .desktop: return .desktop
        case .mobile: return .mobile
        case .tablet: return .tablet
        case .tv: return .tv
        case .vr: return .vr
        case .unknown: return .unknown
        }
    }
}

public enum DeviceCapability {
    case sendTab

    internal static func fromMsg(msg: MsgTypes_Device.Capability) -> DeviceCapability {
        switch msg {
        case .sendTab: return .sendTab
        }
    }

    internal func toMsg() -> MsgTypes_Device.Capability {
        switch self {
        case .sendTab: return .sendTab
        }
    }
}

extension Array where Element == DeviceCapability {
    internal func toCollectionMsg() -> MsgTypes_Capabilities {
        MsgTypes_Capabilities.with {
            $0.capability = self.map { $0.toMsg() }
        }
    }
}

public struct DevicePushSubscription {
    public let endpoint: String
    public let publicKey: String
    public let authKey: String

    public init(endpoint: String, publicKey: String, authKey: String) {
        self.endpoint = endpoint
        self.publicKey = publicKey
        self.authKey = authKey
    }

    internal init(msg: MsgTypes_Device.PushSubscription) {
        endpoint = msg.endpoint
        publicKey = msg.publicKey
        authKey = msg.authKey
    }
}

public enum DeviceEvent {
    case tabReceived(Device?, [TabData])

    internal static func fromCollectionMsg(msg: MsgTypes_AccountEvents) -> [DeviceEvent] {
        msg.events.map { DeviceEvent.fromMsg(msg: $0) }
    }

    internal static func fromMsg(msg: MsgTypes_AccountEvent) -> DeviceEvent {
        switch msg.type {
        case .tabReceived: do {
            let device = msg.tabReceivedData.hasFrom ? Device(msg: msg.tabReceivedData.from) : nil
            let entries = msg.tabReceivedData.entries.map { TabData(title: $0.title, url: $0.url) }
            return .tabReceived(device, entries)
        }
        }
    }
}

public struct TabData {
    public let title: String
    public let url: String
}
