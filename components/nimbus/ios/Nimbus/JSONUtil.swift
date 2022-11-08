/* This Source Code Form is subject to the terms of the Mozilla
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation

internal func stringify(jsonObject: [String: Any]) throws -> String {
    let data = try JSONSerialization.data(withJSONObject: jsonObject)
    guard let s = String(data: data, encoding: .utf8) else {
        throw NimbusError.JsonError(message: "Unable to encode")
    }
    return s
}

internal func toJson(string: String) throws -> [String: Any] {
    guard let data = string.data(using: .utf8) else {
        throw NimbusError.JsonError(message: "Unable to decode string into data")
    }
    let obj = try JSONSerialization.jsonObject(with: data)
    guard let obj = obj as? [String: Any] else {
        throw NimbusError.JsonError(message: "Unable to cast into JSONObject")
    }
    return obj
}
