#!/usr/bin/env python3

# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.


import argparse
import json
import os
import pathlib

import toml

ROOT_DIR = pathlib.Path(__file__).parent.parent.parent


def main():
    args = parse_args()
    dump_json(args)


def dump_json(args):
    data = {
        "version": find_version(),
        "commit": os.environ["APPSERVICES_HEAD_REV"],
    }

    dir = os.path.dirname(args.path)
    if not os.path.exists(dir):
        os.makedirs(dir)
    with open(args.path, "w") as f:
        json.dump(data, f)


def find_version():
    path = ROOT_DIR.joinpath("components", "support", "nimbus-cli", "Cargo.toml")
    with open(path) as f:
        data = toml.load(f)
    return data["package"]["version"]


def parse_args():
    parser = argparse.ArgumentParser(
        description="Generate JSON file with information about the nimbus-cli build"
    )
    parser.add_argument("path")
    return parser.parse_args()


if __name__ == "__main__":
    main()
