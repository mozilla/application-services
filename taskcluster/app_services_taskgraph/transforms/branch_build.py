# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

from taskgraph.transforms.base import TransformSequence

transforms = TransformSequence()


@transforms.add
def setup(config, tasks):
    branch_build_params = config.params.get("branch-build", {})

    for task in tasks:
        if "run" in task:
            run = task["run"]
            if "pre-gradlew" in task["run"]:
                run["pre-gradlew"] = transform_commands(
                    branch_build_params, run["pre-gradlew"]
                )
            if "pre-commands" in task["run"]:
                run["pre-commands"] = transform_commands(
                    branch_build_params, run["pre-commands"]
                )
        yield task


def transform_commands(branch_build_params, command_list):
    return [transform_command(branch_build_params, command) for command in command_list]


def transform_command(branch_build_params, command):
    if command == "setup-branch-build-firefox-android":
        try:
            firefox_android_params = branch_build_params["firefox-android"]
        except KeyError:
            # No branch build params to use for the transform, this task should be filtered out by
            # filter_branch_build_tasks.  In the meantime, return an placeholder value.
            return ["/bin/false"]
        return [
            "taskcluster/scripts/setup-branch-build-firefox-android.py",
            firefox_android_params.get("owner", "mozilla-mobile"),
            firefox_android_params.get("branch", "main"),
        ]
    elif command == "setup-branch-build-firefox-ios":
        try:
            firefox_ios_params = branch_build_params["firefox-ios"]
        except KeyError:
            # No branch build params to use for the transform, this task should be filtered out by
            # filter_branch_build_tasks.  In the meantime, return an placeholder value.
            return ["/bin/false"]
        return [
            "taskcluster/scripts/setup-branch-build-firefox-ios.py",
            firefox_ios_params.get("owner", "mozilla-mobile"),
            firefox_ios_params.get("branch", "main"),
        ]
    else:
        return command
