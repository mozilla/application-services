/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation

extension Dictionary {
   internal func mapKeysNotNull<K1>(_ transform: (Key) -> K1?) -> Dictionary<K1, Value> {
       let transformed: [(K1, Value)] = self.compactMap { k, v in
           transform(k).flatMap { ($0, v) }
       }
       return Dictionary<K1, Value>(uniqueKeysWithValues: transformed)
   }

   internal func mapValuesNotNull<V1>(_ transform: (Value) -> V1?) -> Dictionary<Key, V1> {
       return self.compactMapValues(transform)
   }

   internal func mapNotNull<K1, V1>(_ keyTransform: (Key) -> K1?, _ valueTransform: (Value) -> V1?) -> Dictionary<K1, V1> {
       let transformed: [(K1, V1)] = self.compactMap { k, v in
           guard let k1 = keyTransform(k),
                 let v1 = valueTransform(v) else {
                     return nil
                 }
           return (k1, v1)
       }
       return Dictionary<K1, V1>(uniqueKeysWithValues: transformed)
   }

   internal func mergeWith(_ defaults: [Key:Value], _ valueMerger: ((Value, Value) -> Value)? = nil) -> Dictionary<Key, Value> {
       guard let valueMerger = valueMerger else {
           return self.merging(defaults, uniquingKeysWith: { overide, _ in overide })
       }

       return self.merging(defaults, uniquingKeysWith: valueMerger)
   }
}
