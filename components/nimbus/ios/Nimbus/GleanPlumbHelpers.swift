/* This Source Code Form is subject to the terms of the Mozilla
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation

/**
 * Instances of this class are useful for implementing a messaging service based upon
 * Nimbus.
 */
public protocol GleanPlumbProtocol {
    func createMessageHelper() -> GleanPlumbMessageHelper
}

/**
 * A helper object to make working with Strings uniform across multiple implementations of the messaging
 * system.
 *
 * This object provides access to a JEXL evaluator which runs against the same context as provided by
 * Nimbus targeting.
 *
 * It should also provide a similar function for String substitution, though this scheduled for EXP-2159.
 */
public class GleanPlumbMessageHelper {
    private let targetingHelper: NimbusTargetingHelperProtocol

    init(targetingHelper: NimbusTargetingHelperProtocol) {
        self.targetingHelper = targetingHelper
    }

    public func evalJexl(expression: String) throws -> Bool {
        try targetingHelper.evalJexl(expression: expression, json: nil)
    }

    public func evalJexl(expression: String, json: [String: Any]) throws -> Bool {
        let string = String(data: try JSONSerialization.data(withJSONObject: json, options: []), encoding: .utf8)
        return try targetingHelper.evalJexl(expression: expression, json: string)
    }

    public func evalJexl<T: Encodable>(expression: String, context: T) throws -> Bool {
        let encoder = JSONEncoder()
        encoder.keyEncodingStrategy = .convertToSnakeCase

        let data = try encoder.encode(context)
        let string = String(data: data, encoding: .utf8)!

        return try targetingHelper.evalJexl(expression: expression, json: string)
    }
}

public class AlwaysFalseTargetingHelper: NimbusTargetingHelperProtocol {
    public func evalJexl(expression _: String, json _: String?) throws -> Bool {
        false
    }
}
