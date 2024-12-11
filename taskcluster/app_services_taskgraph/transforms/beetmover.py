# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

import posixpath
from copy import deepcopy

from taskgraph.transforms.base import TransformSequence
from taskgraph.util.dependencies import get_dependencies, get_primary_dependency
from taskgraph.util.schema import resolve_keyed_by

from ..build_config import get_version
from . import publications_to_artifact_map_paths, publications_to_artifact_paths

transforms = TransformSequence()


DESTINATION_PATHS = {
    "promote": ["pub/app-services/candidates/candidates-{version}/build{build}"],
    "ship": ["pub/app-services/releases/{version}"],
}


@transforms.add
def adjust_name(config, tasks):
    for task in tasks:
        dep = get_primary_dependency(config, task)
        if dep.kind == "swift":
            task["name"] = "swift-build"
        yield task


@transforms.add
def resolve_keys(config, tasks):
    for task in tasks:
        for key in ("worker.action",):
            resolve_keyed_by(
                task, key, item_name=task["name"], level=config.params["level"]
            )
        yield task


def _build_upstream_artifacts(config, task):
    upstream_artifacts = []

    for dep in get_dependencies(config, task):
        paths = sorted(
            artifact["name"] for artifact in dep.attributes.get("release-artifacts", [])
        )

        if paths:
            upstream_artifacts.extend(
                [
                    {
                        "taskId": {"task-reference": f"<{dep.kind}>"},
                        "taskType": "repackage",
                        "paths": paths,
                    }
                ]
            )

    return upstream_artifacts


def _build_maven_upstream_artifacts(config, task):
    module_info = task["attributes"]["buildconfig"]
    version = get_version(config.params)
    publications = module_info["publications"]

    return [
        {
            "taskId": {"task-reference": "<module-build>"},
            "taskType": "build",
            "paths": (
                publications_to_artifact_paths(
                    version, publications, ("", ".sha1", ".md5")
                )
            ),
        },
        {
            "taskId": {"task-reference": "<signing>"},
            "taskType": "signing",
            "paths": (publications_to_artifact_paths(version, publications, (".asc",))),
        },
    ]


@transforms.add
def build_upstream_artifacts(config, tasks):
    for task in tasks:
        task.setdefault("worker", {})
        if task["worker"].get("action") == "push-to-maven":
            task["worker"]["upstream-artifacts"] = _build_maven_upstream_artifacts(
                config, task
            )

        else:
            task["worker"]["upstream-artifacts"] = _build_upstream_artifacts(
                config, task
            )

        yield task


@transforms.add
def split_by_release_phase(config, tasks):
    for task in tasks:
        attributes = task.setdefault("attributes", {})
        if attributes.get("shipping_phase"):
            yield task
            continue

        for phase in config.graph_config["release-promotion"]["flavors"]:
            reltask = deepcopy(task)
            reltask["attributes"]["shipping_phase"] = phase
            reltask["name"] = f"{reltask['name']}-{phase}"
            yield reltask


def _build_artifact_map(config, task):
    context = {
        "version": get_version(config.params),
        "build": config.params["moz_build_date"],
    }
    destination_paths = [
        path.format(**context)
        for path in DESTINATION_PATHS[task["attributes"]["shipping_phase"]]
    ]

    artifact_map = []
    for artifact in task["worker"]["upstream-artifacts"]:
        artifact_map.append(
            {
                "task-id": artifact["taskId"],
                "paths": {
                    path: {
                        "destinations": [
                            posixpath.join(
                                destination_path,
                                posixpath.basename(path),
                            )
                            for destination_path in destination_paths
                        ]
                    }
                    for path in artifact["paths"]
                },
                "locale": "multi",
            }
        )
    return artifact_map


def _build_maven_artifact_map(config, task):
    module_info = task["attributes"]["buildconfig"]
    version = get_version(config.params)

    publications = module_info["publications"]

    return [
        {
            "locale": "en-US",
            "task-id": {"task-reference": "<module-build>"},
            "paths": (
                publications_to_artifact_map_paths(
                    version,
                    publications,
                    config.params.get("preview-build"),
                    ("", ".sha1", ".md5"),
                )
            ),
        },
        {
            "locale": "en-US",
            "task-id": {"task-reference": "<signing>"},
            "paths": publications_to_artifact_map_paths(
                version, publications, config.params.get("preview-build"), (".asc",)
            ),
        },
    ]


@transforms.add
def build_artifact_map(config, tasks):
    for task in tasks:
        if task["worker"].get("action") == "push-to-maven":
            task["worker"]["artifact-map"] = _build_maven_artifact_map(config, task)

        else:
            task["worker"]["artifact-map"] = _build_artifact_map(config, task)

        yield task


@transforms.add
def add_remaining_beetmover_config(config, tasks):
    level = config.params["level"]

    for task in tasks:
        shipping_phase = task["attributes"]["shipping_phase"]
        task["worker"]["version"] = get_version(config.params)

        if task["worker"].get("action") == "push-to-maven":
            task["name"] = f"module-{task['name']}"
            task["description"] = task["description"].format(
                task["attributes"]["buildconfig"]["name"]
            )
            task["worker"]["bucket"] = (
                "maven-production" if level == "3" else "maven-staging"
            )

        else:
            task["description"] = task["description"].format(task["name"])
            task["worker"]["bucket"] = "release" if level == "3" else "dep"

            if shipping_phase == "ship":
                task["worker"]["action"] = "direct-push-to-bucket"
            else:
                task["worker"]["action"] = "push-to-candidates"

        yield task
