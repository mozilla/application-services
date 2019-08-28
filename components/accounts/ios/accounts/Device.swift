/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation

public struct Device {
    let id: String
    let displayName: String
    let type: DeviceType
    let pushSubscription: PushSubscription?
    let pushEndpointExpired: Bool
    let isCurrentDevice: Bool
    let lastAccessTime: UInt64?
    let capabilities: [Capability]

    internal init(msg: MsgTypes_Device) {
        id = msg.id
        displayName = msg.displayName
        type = DeviceType(msg: msg.type)
        pushSubscription = msg.hasPushSubscription ? PushSubscription(msg: msg.pushSubscription) : nil
        pushEndpointExpired = msg.pushEndpointExpired
        isCurrentDevice = msg.isCurrentDevice
        lastAccessTime = msg.hasLastAccessTime ? msg.lastAccessTime : nil
        capabilities = msg.capabilities.map { Capability(msg: $0) }
    }

    public enum DeviceType {
        case Desktop
        case Mobile
        case Tablet
        case VR
        case TV
        case Unknown

        internal init(msg: MsgTypes_Device.TypeEnum) {
            switch msg {
            case .desktop: self = .Desktop
            case .mobile: self = .Mobile
            case .tablet: self = .Tablet
            case .vr: self = .VR
            case .tv: self = .TV
            default: self = .Unknown
            }
        }

        internal func toMessage() -> MsgTypes_Device.TypeEnum {
            switch self {
            case .Desktop: return .desktop
            case .Mobile: return .mobile
            case .Tablet: return .tablet
            case .VR: return .vr
            case .TV: return .tv
            default: return .unknown
            }
        }
    }

    public struct PushSubscription {
        let endpoint: String
        let publicKey: String
        let authKey: String

        internal init(msg: MsgTypes_Device.PushSubscription) {
            endpoint = msg.endpoint
            publicKey = msg.publicKey
            authKey = msg.authKey
        }
    }

    public enum Capability {
        case SendTab

        internal init(msg: MsgTypes_Device.Capability) {
            switch msg {
            case .sendTab: self = .SendTab
            }
        }

        internal func toMessage() -> MsgTypes_Device.Capability {
            switch self {
            case .SendTab: return MsgTypes_Device.Capability.sendTab
            }
        }
    }
}

public struct ConstellationState {
    let currentDevice: Device?
    let otherDevices: [Device]

    internal init(msg: MsgTypes_DeviceConstellation) {
        currentDevice = msg.hasCurrentDevice ? Device(msg: msg.currentDevice) : nil
        otherDevices = msg.otherDevices.devices.map { Device(msg: $0) }
    }
}

public enum DeviceEvent {
    public struct TabData {
        let title: String
        let url: String

        internal init(msg: MsgTypes_AccountEvent.TabReceivedData.TabHistoryEntry) {
            title = msg.title
            url = msg.url
        }
    }

    case TabReceived(from: Device?, entries: [TabData])

    internal init(msg: MsgTypes_AccountEvent) {
        switch msg.type {
        case .tabReceived: self = .TabReceived(from: Device(msg: msg.tabReceivedData.from), entries: msg.tabReceivedData.entries.map { TabData(msg: $0) })
        }
    }
}

public enum DeviceEventOutgoing {
    case SendTab(title: String, url: String)
}

public struct DeviceConfig {
    let name: String
    let type: Device.DeviceType
    let capabilities: [Device.Capability]
}
