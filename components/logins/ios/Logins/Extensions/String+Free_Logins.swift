/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation

/*
 * NOTE: This is now provided by Glean.
 * Because it's all compiled into a single libmegazord.a it doesn't matter which `destroy_string` we call,
 * internally it defers to the same string implementation.
 * Because Glean is added as a submodule it's easier to change this occurence than it it to change the one of Glean.
 */
/*
 extension String {
     public init(freeingRustString rustString: UnsafeMutablePointer<CChar>) {
         defer { sync15_passwords_destroy_string(rustString) }
         self.init(cString: rustString)
     }
 }
 */
