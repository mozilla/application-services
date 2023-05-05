# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.


from taskgraph.transforms.base import TransformSequence

from ..build_config import get_version

transforms = TransformSequence()

@transforms.add
def transform_routes(config, tasks):
    version = get_version(config.params)
    for task in tasks:
        task["routes"] = [
            route.replace("{appservices_version}", version)
            for route in task.get("routes", [])
        ]
        yield task

