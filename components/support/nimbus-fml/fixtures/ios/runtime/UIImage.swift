/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */
import Foundation
public class UIImage {
    private var named: String
    private var bundle: Bundle
    private var compatibleWith: String?
    public init?(named: String, in bundle: Bundle, compatibleWith: String?) {
        self.named = named
        self.bundle = bundle
        self.compatibleWith = compatibleWith
    }
}