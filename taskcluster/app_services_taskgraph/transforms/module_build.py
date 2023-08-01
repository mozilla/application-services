# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.


from taskgraph.transforms.base import TransformSequence

from ..build_config import get_version, get_extensions


transforms = TransformSequence()

@transforms.add
def rustup_setup(config, tasks):
    for task in tasks:
        task["run"].setdefault("pre-gradlew", [])
        task["run"]["pre-gradlew"].insert(0,
            [
                "git",
                "submodule",
                "update",
                "--init",
            ]
        )
        yield task


@transforms.add
def build_task(config, tasks):
    if config.params.get("preview-build") is None:
        path_prefix = "/builds/worker/checkouts/vcs/build/maven/org/mozilla/appservices/"
    else:
        path_prefix = "/builds/worker/checkouts/vcs/build/maven/org/mozilla/appservices/nightly"

    for task in tasks:
        module_info = task["attributes"]["buildconfig"]
        name = module_info["name"]
        version = get_version(config.params)

        for i,item in enumerate(task["run"]["gradlew"]):
            task["run"]["gradlew"][i] = task["run"]["gradlew"][i].format(module_name=name)
        if config.params.get('preview-build') is not None:
            task["run"]["gradlew"].append(f"-PnightlyVersion={version}")
        task["description"] = task["description"].format(module_name=name)
        task["worker"]["artifacts"] = artifacts = []

        all_extensions = get_extensions(name)
        for publication_name, extensions in all_extensions.items():
            for extension in extensions:
                artifact_filename = f"{publication_name}-{version}{extension}"
                artifacts.append({
                    "name": f"public/build/{artifact_filename}",
                    "path": f"{path_prefix}/{publication_name}/{version}/{artifact_filename}",
                    "type": "file",
                })

        yield task
