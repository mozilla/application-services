/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import FeatureManifest
import Foundation

class Feature: FMLFeatureInterface {
    let string: String
    init(_ string: String) {
        self.string = string
    }
}

let queue: OperationQueue = {
    let queue = OperationQueue()
    queue.maxConcurrentOperationCount = 5
    return queue
}()

let api: FeaturesInterface = HardcodedNimbusFeatures(with: ["test-feature-holder": "{}"])
let holder = FeatureHolder<Feature>({ api }, featureId: "test-feature-holder") { _, _ in Feature("NO CRASH") }

for _ in 1 ..< 10000 {
    queue.addOperation {
        let _ = holder.value()
    }
}
