/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation

extension Dictionary {
    func mapKeys<K1>(_ transform: (Key) -> K1?) -> Dictionary<K1, Value> {
        let filtered: Dictionary<Key, Value> = self.filter() { (key, value) -> Bool in
            return transform(key) != nil
        }
        let mapped = filtered.map() { (key, value) -> (K1, Value) in (transform(key)!, value)}
        return Dictionary<K1, Value>(uniqueKeysWithValues: mapped)
    }
    
    func mapValuesNotNull<V1>(_ transform: (Value) -> V1?) -> Dictionary<Key, V1> {
        let filtered: Dictionary<Key, Value> = self.filter() { (key, value) -> Bool in
            return transform(value) != nil
        }
        return filtered.mapValues() { (value) -> V1 in transform(value)!}
    }
    
    func mapNotNull<K1, V1>(_ keyTransform: (Key) -> K1?, _ valueTransform: (Value) -> V1?) -> Dictionary<K1, V1> {
        let filtered: Dictionary<Key, Value> = self.filter() { (key, value) -> Bool in
            return keyTransform(key) != nil && valueTransform(value) != nil
        }
        let mapped = filtered.map() { (key, value) -> (K1, V1) in (keyTransform(key)!, valueTransform(value)!)}
        return Dictionary<K1, V1>(uniqueKeysWithValues: mapped)
    }
    
    func mergeWith(_ defaults: [Key:Value], _ valueTransform: ((Value, Value) -> Value?)? = nil) -> Dictionary<Key, Value> {
        var target: Dictionary<Key, Value> = [:]
        defaults.forEach() { (key, value) in
            target[key] = value
        }
        self.forEach() { (key, value) in
            var override: Value? = nil
            if let defaultValue = defaults[key] {
                override = valueTransform?(value, defaultValue)
            }
            if let override = override {
                target[key] = override
            } else {
                target[key] = value
            }
        }
        return target
    }
}
