/* This Source Code Form is subject to the terms of the Mozilla
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation

/// `Variables` provides a type safe key-value style interface to configure application features
///
/// The feature developer requests a typed value with a specific `key`. If the key is present, and
/// the value is of the correct type, then it is returned. If neither of these are true, then `null`
/// is returned.
///
/// The values may be under experimental control, but if not, `nil` is returned. In this case, the app should
/// provide the default value.
///
/// ```
/// let variables = nimbus.getVariables("about_welcome")
///
/// let title = variables.getString("title") ?? "Welcome, oo vudge"
/// let numSections = variables.getInt("num-sections") ?? 2
/// let isEnabled = variables.getBool("isEnabled") ?? true
/// ```
///
/// This may become the basis of a generated-from-manifest solution.
public protocol Variables {
    func getString(_ key: String) -> String?
    func getInt(_ key: String) -> Int?
    func getBool(_ key: String) -> Bool?

    // Get a child configuration object.
    func getVariables(_ key: String) -> Variables?
}

public extension Variables {
    // This may be important when transforming in to a code generated object.
    func getVariables<T>(_ key: String, transform: (Variables) -> T) -> T? {
        if let value = getVariables(key) {
            return transform(value)
        } else {
            return nil
        }
    }
}

/// A thin wrapper around the JSON produced by the `get_feature_variables_json(feature_id)` call, useful
/// for configuring a feature, but without needing the developer to know about experiment specifics.
internal class JSONVariables: Variables {
    private let json: [String: Any]

    init(with json: [String: Any]) {
        self.json = json
    }

    // These `get*` methods get values from the wrapped JSON object, and transform them using the
    // `as*` methods.
    func getString(_ key: String) -> String? {
        return value(key)
    }

    func getInt(_ key: String) -> Int? {
        return value(key)
    }

    func getBool(_ key: String) -> Bool? {
        return value(key)
    }

    // Methods used to get sub-objects. We immediately re-wrap an JSON object if it exists.
    func getVariables(_ key: String) -> Variables? {
        if let dictionary: [String: Any] = value(key) {
            return JSONVariables(with: dictionary)
        } else {
            return nil
        }
    }

    private func value<T>(_ key: String) -> T? {
        return json[key] as? T
    }
}

// Another implementation of `Variables` may just return null for everything.
class NilVariables: Variables {
    static let instance: Variables = NilVariables()

    func getString(_: String) -> String? {
        return nil
    }

    func getInt(_: String) -> Int? {
        return nil
    }

    func getBool(_: String) -> Bool? {
        return nil
    }

    func getVariables(_: String) -> Variables? {
        return nil
    }
}
