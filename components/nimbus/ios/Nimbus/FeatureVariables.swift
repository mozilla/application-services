/* This Source Code Form is subject to the terms of the Mozilla
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation
import UIKit

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
    /// Finds a string typed value for this key. If none exists, `null` is returned.
    ///
    /// N.B. the `key` and type `String` should be listed in the experiment manifest.
    func getString(_ key: String) -> String?

    /// Finds a integer typed value for this key. If none exists, `null` is returned.
    ///
    /// N.B. the `key` and type `Int` should be listed in the experiment manifest.
    func getInt(_ key: String) -> Int?

    /// Finds a boolean typed value for this key. If none exists, `null` is returned.
    ///
    /// N.B. the `key` and type `String` should be listed in the experiment manifest.
    func getBool(_ key: String) -> Bool?

    /// Uses `getString(key: String)` to find the name of a drawable resource. If no value for `key`
    /// exists, or no resource named with that value exists, then `null` is returned.
    ///
    /// N.B. the `key` and type `Image` should be listed in the experiment manifest. The
    /// names of the drawable resources should also be listed.
    func getImage(_ key: String) -> UIImage?

    /// Uses `getString(key: String)` to find the name of a string resource. If a value exists, and
    /// a string resource exists with that name, then returns the string from the resource. If no
    /// such resource exists, then return the string value as the text.
    ///
    /// For strings, this is almost always the right choice.
    ///
    /// N.B. the `key` and type `LocalizedString` should be listed in the experiment manifest. The
    /// names of the string resources should also be listed.
    func getText(_ key: String) -> String?

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

protocol VariablesWithBundle: Variables {
    var resourceBundles: [Bundle] { get }
}

extension VariablesWithBundle {
    func getImage(_ key: String) -> UIImage? {
        return lookup(key, transform: asImage)
    }

    func getText(_ key: String) -> String? {
        return lookup(key, transform: asLocalizedString)
    }

    private func lookup<T>(_ key: String, transform: (String) -> T?) -> T? {
        guard let value = getString(key) else {
            return nil
        }
        return transform(value)
    }

    /// Search through the resource bundles looking for an image of the given name.
    ///
    /// If no image is found in any of the `resourceBundles`, then the `nil` is returned.
    func asImage(name: String) -> UIImage? {
        for bundle in resourceBundles {
            if let image = UIImage(named: name, in: bundle, compatibleWith: nil) {
                return image
            }
        }
        return nil
    }

    /// Search through the resource bundles looking for localized strings with the given name.
    /// If the `name` contains exactly one slash, it is split up and the first part of the string is used
    /// as the `tableName` and the second the `key` in localized string lookup.
    /// If no string is found in any of the `resourceBundles`, then the `name` is passed back unmodified.
    func asLocalizedString(name: String) -> String? {
        let parts = name.split(separator: "/", maxSplits: 1, omittingEmptySubsequences: true).map { String($0) }
        let key: String
        let tableName: String?
        switch parts.count {
        case 2:
            tableName = parts[0]
            key = parts[1]
        default:
            tableName = nil
            key = name
        }

        for bundle in resourceBundles {
            let value = bundle.localizedString(forKey: key, value: nil, table: tableName)
            if value != key {
                return value
            }
        }
        return name
    }
}

/// A thin wrapper around the JSON produced by the `get_feature_variables_json(feature_id)` call, useful
/// for configuring a feature, but without needing the developer to know about experiment specifics.
internal class JSONVariables: VariablesWithBundle {
    private let json: [String: Any]
    internal let resourceBundles: [Bundle]

    init(with json: [String: Any], in bundles: [Bundle] = [Bundle.main]) {
        self.json = json
        resourceBundles = bundles
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
            return JSONVariables(with: dictionary, in: resourceBundles)
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

    func getImage(_: String) -> UIImage? {
        return nil
    }

    func getText(_: String) -> String? {
        return nil
    }

    func getVariables(_: String) -> Variables? {
        return nil
    }
}
