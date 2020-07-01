# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

from __future__ import absolute_import, print_function, unicode_literals

from taskgraph.transforms.base import TransformSequence

from ..build_config import get_version, get_extensions


transforms = TransformSequence()


@transforms.add
def build_task(config, tasks):
    for task in tasks:
        task["worker"]["artifacts"] = [{
            "name": "public/build/build_resources.json",
            "path": "/builds/worker/checkouts/src/build_resources.json",
            "type": "file",
        }]

        yield task
