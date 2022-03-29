/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation

public extension Dictionary {
    func mapKeysNotNull<K1>(_ transform: (Key) -> K1?) -> [K1: Value] {
        let transformed: [(K1, Value)] = compactMap { k, v in
            transform(k).flatMap { ($0, v) }
        }
        return [K1: Value](uniqueKeysWithValues: transformed)
    }

    func mapValuesNotNull<V1>(_ transform: (Value) -> V1?) -> [Key: V1] {
        return compactMapValues(transform)
    }

    func mapNotNull<K1, V1>(_ keyTransform: (Key) -> K1?, _ valueTransform: (Value) -> V1?) -> [K1: V1] {
        let transformed: [(K1, V1)] = compactMap { k, v in
            guard let k1 = keyTransform(k),
                  let v1 = valueTransform(v)
            else {
                return nil
            }
            return (k1, v1)
        }
        return [K1: V1](uniqueKeysWithValues: transformed)
    }

    func mergeWith(_ defaults: [Key: Value], _ valueMerger: ((Value, Value) -> Value)? = nil) -> [Key: Value] {
        guard let valueMerger = valueMerger else {
            return merging(defaults, uniquingKeysWith: { overide, _ in overide })
        }

        return merging(defaults, uniquingKeysWith: valueMerger)
    }
}

public extension Array where Element == Bundle {
    /// Search through the resource bundles looking for an image of the given name.
    ///
    /// If no image is found in any of the `resourceBundles`, then the `nil` is returned.
    func getImage(named name: String) -> UIImage? {
        for bundle in self {
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
    func getString(named name: String) -> String? {
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

        for bundle in self {
            let value = bundle.localizedString(forKey: key, value: nil, table: tableName)
            if value != key {
                return value
            }
        }
        return nil
    }
}
