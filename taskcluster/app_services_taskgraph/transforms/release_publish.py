# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.


from taskgraph.transforms.base import TransformSequence

from ..build_config import get_version
from ..beetmover import get_maven_bucket

transforms = TransformSequence()

@transforms.add
def setup_command(config, tasks):
    version = get_version(config.params)
    if config.params['level'] == '3':
        if config.params.get('preview-build') is None:
            maven_channel = "maven-production"
        else:
            maven_channel = "maven-nightly-production"
    else:
        if config.params.get('preview-build') is None:
            maven_channel = "maven-staging"
        else:
            maven_channel = "maven-nightly-staging"
    release_type = config.params.get('release', 'nightly')

    for task in tasks:
        task["run"]["commands"] = [
           [
               "/builds/worker/checkouts/vcs/taskcluster/scripts/generate-release-json.py",
               f"/builds/worker/checkouts/vcs/build/{release_type}.json",
               "--version", version,
               "--maven-channel", maven_channel,
           ]
        ]
        task['worker']['artifacts'] = [
            {
                "name": f"public/build/{release_type}.json",
                "path": f"/builds/worker/checkouts/vcs/build/{release_type}.json",
                "type": "file",
            }
        ]
        task["routes"] = [
            f"index.project.application-services.v2.{release_type}.latest",
            f"index.project.application-services.v2.{release_type}.{version}",
        ]
        yield task
