 /* This Source Code Form is subject to the terms of the Mozilla Public
  * License, v. 2.0. If a copy of the MPL was not distributed with this
  * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation

// We use a serial queue to protect access to the rust object.
let queue = DispatchQueue(label: "com.mozilla.rustappservices")

