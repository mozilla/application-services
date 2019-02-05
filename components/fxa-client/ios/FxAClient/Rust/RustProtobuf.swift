/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation
import SwiftProtobuf

open class RustProtobuf<T: SwiftProtobuf.Message> {
    var raw: T

    init(raw: ByteBuffer) {
        if let data = raw.data {
            let bytes = Data.init(bytes: data, count: Int(raw.len))
            self.raw = try! T(serializedData: bytes)
        } else {
            self.raw = T.init()
        }
        fxa_bytebuffer_free(raw)
    }
}
