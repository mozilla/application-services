# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.


from taskgraph.transforms.base import TransformSequence

from ..build_config import get_version

transforms = TransformSequence()


@transforms.add
def add_release_routes(config, tasks):
    for task in tasks:
        # Add routes listed in `release-routes` if we're building for a release
        release_routes = task.get("attributes", {}).get("release-routes")
        release_type = config.params.get("release-type")
        if release_type and release_routes:
            task.setdefault("routes", []).extend(release_routes)
        yield task


@transforms.add
def transform_routes(config, tasks):
    version = get_version(config.params)
    for task in tasks:
        task["routes"] = [
            route.format(appservices_version=version, **config.params)
            for route in task.get("routes", [])
        ]
        yield task
