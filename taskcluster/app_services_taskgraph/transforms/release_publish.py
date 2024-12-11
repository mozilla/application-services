# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.


from taskgraph.transforms.base import TransformSequence

from ..build_config import get_version

transforms = TransformSequence()


@transforms.add
def setup_command(config, tasks):
    version = get_version(config.params)
    instance = "production" if config.params["level"] == "3" else "staging"
    nightly = "-nightly" if config.params.get("preview-build") else ""
    maven_channel = f"maven{nightly}-{instance}"
    release_type = config.params.get("release-type", "nightly")
    head_rev = config.params["head_rev"]

    for task in tasks:
        task["run"]["commands"] = [
            [
                "/builds/worker/checkouts/vcs/taskcluster/scripts/generate-release-json.py",
                f"/builds/worker/checkouts/vcs/build/{release_type}.json",
                "--version",
                version,
                "--maven-channel",
                maven_channel,
            ]
        ]
        task["worker"]["artifacts"] = [
            {
                "name": f"public/build/{release_type}.json",
                "path": f"/builds/worker/checkouts/vcs/build/{release_type}.json",
                "type": "file",
            }
        ]
        if config.params["level"] == "3":
            task["routes"] = [
                f"index.project.application-services.v2.{release_type}.latest",
                f"index.project.application-services.v2.{release_type}.{version}",
                f"index.project.application-services.v2.{release_type}.revision.{head_rev}",
            ]
        yield task


@transforms.add
def convert_dependencies(config, tasks):
    """
    Convert kind dependencies into soft-dependencies

    The `release-publish` task lists the `build-summary` task as a kind dependency, but we need a
    transform to setup the actual dependency.  When
    https://github.com/taskcluster/taskgraph/pull/236 is merged, we could simplify this code.
    """
    for task in tasks:
        task.setdefault("soft-dependencies", [])
        task["soft-dependencies"] += [
            dep_task.label for dep_task in config.kind_dependencies_tasks.values()
        ]
        yield task
