# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.


import functools
import subprocess

from taskgraph.transforms.base import TransformSequence
from taskgraph.util.schema import resolve_keyed_by

transforms = TransformSequence()


# TODO: Bug 1637695 to be removed once we retire these old indexes
TOOLCHAIN_OLD_INDEX = {
    "android": "index.project.application-services.application-services.build.libs.android.{sha}",
    "desktop-linux": "index.project.application-services.application-services.build.libs.desktop.linux.{sha}",
    "desktop-macos": "index.project.application-services.application-services.build.libs.desktop.macos.{sha}",
}


@functools.cache
def git_sha_for_directory(directory):
    output = subprocess.check_output(["git", "rev-parse", f"HEAD:{directory}"])
    sha = output.decode("utf8").strip()
    return sha


@transforms.add
def resolve_keys(config, tasks):
    for task in tasks:
        resolve_keyed_by(
            task,
            "routes",
            item_name=task["name"],
            **{"tasks-for": config.params["tasks_for"]},
        )
        # TODO: Bug 1637695 - temp solution to unblock local building of
        # application-services. Once we switch to new indexes, we should clean this up
        if task["name"] in TOOLCHAIN_OLD_INDEX.keys() and config.params["level"] == "3":
            sha = git_sha_for_directory("libs")
            routes = task["routes"]
            routes.append(TOOLCHAIN_OLD_INDEX[task["name"]].format(sha=sha))

        yield task
