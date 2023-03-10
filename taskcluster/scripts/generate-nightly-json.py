#!/usr/bin/env python3

# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.


import argparse
import json
import os

def main():
    args = parse_args()
    dump_json(args)

def dump_json(args):
    data = {
        'version': args.version,
        'channel': args.maven_channel,
        'commit': os.environ['APPSERVICES_HEAD_REV'],
    }
    dir = os.path.dirname(args.path)
    if not os.path.exists(dir):
        os.makedirs(dir)
    with open(args.path, "wt") as f:
        json.dump(data, f)

def parse_args():
    parser = argparse.ArgumentParser(description='Publish information about the nightly build')
    parser.add_argument('path')
    parser.add_argument('--version', help='version string', required=True)
    parser.add_argument('--maven-channel', help='channel the maven packages were uploaded to', required=True)
    return parser.parse_args()

if __name__ == '__main__':
    main()
