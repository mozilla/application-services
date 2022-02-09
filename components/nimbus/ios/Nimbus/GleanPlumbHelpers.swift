/* This Source Code Form is subject to the terms of the Mozilla
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation

/**
 * Instances of this class are useful for implementing a messaging service based upon
 * Nimbus.
 */
public protocol GleanPlumbProtocol {
    func createMessageHelper() throws -> GleanPlumbMessageHelper
    func createMessageHelper(_ additionalContext: [String: Any]) throws -> GleanPlumbMessageHelper
    func createMessageHelper<T: Codable>(_ additionalContext: T) throws -> GleanPlumbMessageHelper
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
    private let stringHelper: NimbusStringHelperProtocol

    init(targetingHelper: NimbusTargetingHelperProtocol, stringHelper: NimbusStringHelperProtocol) {
        self.targetingHelper = targetingHelper
        self.stringHelper = stringHelper
    }

    public func evalJexl(expression: String) throws -> Bool {
        try targetingHelper.evalJexl(expression: expression)
    }

    public func getUuid(template: String) -> String? {
        stringHelper.getUuid(template: template)
    }

    public func stringFormat(template: String, uuid: String?) -> String {
        stringHelper.stringFormat(template: template, uuid: uuid)
    }
}

internal class AlwaysFalseTargetingHelper: NimbusTargetingHelperProtocol {
    public func evalJexl(expression _: String) throws -> Bool {
        false
    }
}

internal class NonStringHelper: NimbusStringHelperProtocol {
    public func getUuid(template: String) -> String? {
        nil
    }

    public func stringFormat(template: String, uuid: String?) -> String {
        template
    }
}
