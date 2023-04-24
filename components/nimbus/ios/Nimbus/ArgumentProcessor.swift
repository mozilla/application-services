/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation

enum ArgumentProcessor {
    static func initializeTooling(nimbus: NimbusInterface, args: CliArgs) {
        if args.resetDatabase {
            nimbus.resetEnrollmentsDatabase().waitUntilFinished()
        }
        if let experiments = args.experiments {
            nimbus.setExperimentsLocally(experiments)
            nimbus.applyPendingExperiments().waitUntilFinished()
            // setExperimentsLocally and applyPendingExperiments run on the
            // same single threaded dispatch queue, so we can run them in series,
            // and wait for the apply.
            nimbus.setFetchEnabled(false)
        }
    }

    static func createCommandLineArgs(args: [String]?) -> CliArgs? {
        guard let args = args else {
            return nil
        }
        if !args.contains("--nimbus-cli") {
            return nil
        }

        var argMap = [String: String]()
        var key: String?
        var resetDatabase = false

        args.forEach { arg in
            var value: String?
            switch arg {
            case "--version":
                key = "version"
            case "--experiments":
                key = "experiments"
            case "--reset-db":
                resetDatabase = true
            default:
                value = arg.replacingOccurrences(of: "&apos;", with: "'")
            }

            if let k = key, let v = value {
                argMap[k] = v
                key = nil
                value = nil
            }
        }

        if argMap["version"] != "1" {
            return nil
        }

        let experiments = argMap["experiments"]?.map { (string: String) -> String? in
            guard let payload = try? Dictionary.parse(jsonString: string), payload["data"] is [Any] else {
                return nil
            }
            return string
        }

        return CliArgs(resetDatabase: resetDatabase, experiments: experiments)
    }
}

struct CliArgs: Equatable {
    let resetDatabase: Bool
    let experiments: String?
}
