#!/bin/env python
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

from __future__ import print_function

import json
import os
import redo
import requests
import shutil
import sys
from optparse import OptionParser

DEFAULT_SYMBOL_URL = "https://symbols.mozilla.org/upload/"
MAX_RETRIES = 5

def upload_symbols(zip_file, token_file):
    print("Uploading symbols file '{0}' to '{1}'".format(zip_file, DEFAULT_SYMBOL_URL), file=sys.stdout)
    zip_name = os.path.basename(zip_file)

    # XXX: fetch the symbol upload token from local file, taskgraph handles
    # already that communication with Taskcluster to get the credentials for
    # communicating with the server
    auth_token = ''
    with open(token_file, 'r') as f:
        auth_token = f.read().strip()
    if len(auth_token) == 0:
        print("Failed to get the symbol token.", file=sys.stderr)
    if auth_token == 'faketoken':
        print("'faketoken` detected, not pushing anything", file=sys.stdout)
        sys.exit(0)

    for i, _ in enumerate(redo.retrier(attempts=MAX_RETRIES), start=1):
        print("Attempt %d of %d..." % (i, MAX_RETRIES))
        try:
            if zip_file.startswith("http"):
                zip_arg = {"data": {"url", zip_file}}
            else:
                zip_arg = {"files": {zip_name: open(zip_file, 'rb')}}
            r = requests.post(
                DEFAULT_SYMBOL_URL,
                headers={"Auth-Token": auth_token},
                allow_redirects=False,
                # Allow a longer read timeout because uploading by URL means the server
                # has to fetch the entire zip file, which can take a while. The load balancer
                # in front of symbols.mozilla.org has a 300 second timeout, so we'll use that.
                timeout=(10, 300),
                **zip_arg)
            # 500 is likely to be a transient failure.
            # Break out for success or other error codes.
            if r.status_code < 500:
                break
            print("Error: {0}".format(r), file=sys.stderr)
        except requests.exceptions.RequestException as e:
            print("Error: {0}".format(e), file=sys.stderr)
        print("Retrying...", file=sys.stdout)
    else:
        print("Maximum retries hit, giving up!", file=sys.stderr)
        return False

    if r.status_code >= 200 and r.status_code < 300:
        print("Uploaded successfully", file=sys.stdout)
        return True

    print("Upload symbols failed: {0}".format(r), file=sys.stderr)
    return False

def main():
    parser = OptionParser(usage="usage: <symbol store path>")
    parser.add_option('-t', '--tokenfile', dest='token_file',
                      help='upload symbols token file', default='.symbols_upload_token')
    (options, args) = parser.parse_args()

    if len(args) < 1:
        parser.error("not enough arguments")
        exit(1)

    symbol_path = args[0]
    token_file = options.token_file
    shutil.make_archive(symbol_path , "zip", symbol_path)
    upload_symbols(symbol_path + ".zip", token_file)

# run main if run directly
if __name__ == "__main__":
    main()
