/* This Source Code Form is subject to the terms of the Mozilla
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation
import Glean

/**
 * Instances of this class are useful for implementing a messaging service based upon
 * Nimbus.
 *
 * The message helper is designed to help string interpolation and JEXL evalutaiuon against the context
 * of the attrtibutes Nimbus already knows about.
 *
 * App-specific, additional context can be given at creation time.
 *
 * The helpers are designed to evaluate multiple messages at a time, however: since the context may change
 * over time, the message helper should not be stored for long periods.
 */
public protocol NimbusMessagingProtocol {
    func createMessageHelper() throws -> NimbusMessagingHelperProtocol
    func createMessageHelper(additionalContext: [String: Any]) throws -> NimbusMessagingHelperProtocol
    func createMessageHelper<T: Encodable>(additionalContext: T) throws -> NimbusMessagingHelperProtocol
}

// Deprecated the name GleanPlumb.
public typealias GleanPlumbProtocol = NimbusMessagingProtocol
public typealias GleanPlumbMessageHelper = NimbusMessagingHelper
public typealias NimbusMessagingHelperProtocol = NimbusTargetingHelperProtocol & NimbusStringHelperProtocol

/**
 * A helper object to make working with Strings uniform across multiple implementations of the messaging
 * system.
 *
 * This object provides access to a JEXL evaluator which runs against the same context as provided by
 * Nimbus targeting.
 *
 * It should also provide a similar function for String substitution, though this scheduled for EXP-2159.
 */
public class NimbusMessagingHelper: NimbusMessagingHelperProtocol {
    private let targetingHelper: NimbusTargetingHelperProtocol
    private let stringHelper: NimbusStringHelperProtocol

    public init(targetingHelper: NimbusTargetingHelperProtocol, stringHelper: NimbusStringHelperProtocol) {
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

// MARK: Dummy implementations

internal class AlwaysConstantTargetingHelper: NimbusTargetingHelperProtocol {
    private let constant: Bool

    public init(constant: Bool = false) {
        self.constant = constant
    }

    public func evalJexl(expression _: String) throws -> Bool {
        constant
    }
}

internal class EchoStringHelper: NimbusStringHelperProtocol {
    public func getUuid(template _: String) -> String? {
        nil
    }

    public func stringFormat(template: String, uuid _: String?) -> String {
        template
    }
}
