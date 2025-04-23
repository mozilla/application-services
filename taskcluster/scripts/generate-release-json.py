#!/usr/bin/env python3

# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.


import argparse
import json
import os
from urllib.parse import quote_plus


def main():
    args = parse_args()
    dump_json(args)


def indexed_artifact_url(index_name, filename):
    return "https://firefox-ci-tc.services.mozilla.com/api/index/v1/task/{}/artifacts/public{}".format(
        index_name, quote_plus(f"/build/{filename}")
    )


def dump_json(args):
    data = {
        "version": args.version,
        "channel": args.maven_channel,
        "commit": os.environ["APPSERVICES_HEAD_REV"],
        "nimbus-fml.zip": indexed_artifact_url(
            f"project.application-services.v2.nimbus-fml.{args.version}",
            "nimbus-fml.zip",
        ),
        "nimbus-fml.sha256": indexed_artifact_url(
            f"project.application-services.v2.nimbus-fml.{args.version}",
            "nimbus-fml.sha256",
        ),
        "FocusRustComponents.xcframework.zip": indexed_artifact_url(
            f"project.application-services.v2.swift.{args.version}",
            "FocusRustComponents.xcframework.zip",
        ),
        "MozillaRustComponents.xcframework.zip": indexed_artifact_url(
            f"project.application-services.v2.swift.{args.version}",
            "MozillaRustComponents.xcframework.zip",
        ),
        "swift-components.tar.xz": indexed_artifact_url(
            f"project.application-services.v2.swift.{args.version}",
            "swift-components.tar.xz",
        ),
    }

    dir = os.path.dirname(args.path)
    if not os.path.exists(dir):
        os.makedirs(dir)
    with open(args.path, "w") as f:
        json.dump(data, f)


def parse_args():
    parser = argparse.ArgumentParser(
        description="Publish information about the release builds"
    )
    parser.add_argument("path")
    parser.add_argument("--version", help="version string", required=True)
    parser.add_argument(
        "--maven-channel",
        help="channel the maven packages were uploaded to",
        required=True,
    )
    return parser.parse_args()


if __name__ == "__main__":
    main()
