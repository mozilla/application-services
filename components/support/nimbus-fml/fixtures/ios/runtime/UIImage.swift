/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */
import Foundation
public class UIImage {
    public var name: String
    private var bundle: Bundle
    private var compatibleWith: String?
    public init?(named name: String, in bundle: Bundle, compatibleWith: String?) {
        self.name = name
        self.bundle = bundle
        self.compatibleWith = compatibleWith
    }

    public var accessibilityIdentifier: String?
}

public class Bundle {
    public static let main = Bundle()

    public func localizedString(forKey key: String, value: String?, table: String?) -> String {
        return key
    }
}