# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.


from taskgraph.transforms.base import TransformSequence
from taskgraph.util.schema import resolve_keyed_by

from . import (
    publications_to_artifact_paths, publications_to_artifact_map_paths
)
from ..build_config import get_version

transforms = TransformSequence()


@transforms.add
def build_upstream_artifacts(config, tasks):
    for task in tasks:
        module_info = task["attributes"]["buildconfig"]
        name = module_info["name"]
        version = get_version()
        publications = module_info["publications"]

        worker_definition = {"upstream-artifacts": [{
            "taskId": {"task-reference": "<module-build>"},
            "taskType": "build",
            "paths": (publications_to_artifact_paths(name, version, publications,
                                                     ("", ".sha1", ".md5"))),
        }, {
            "taskId": {"task-reference": "<signing>"},
            "taskType": "signing",
            "paths": (publications_to_artifact_paths(name, version, publications, (".asc",))),
        }]}

        task.setdefault("worker", {})
        task["worker"].update(worker_definition)
        yield task


@transforms.add
def build_artifact_map(config, tasks):
    for task in tasks:
        module_info = task["attributes"]["buildconfig"]
        name = module_info["name"]
        version = get_version()

        publications = module_info["publications"]

        artifact_map = [{
            "locale": "en-US",
            "task-id": {"task-reference": "<module-build>"},
            "paths": (publications_to_artifact_map_paths(name, version, publications,
                                                         ("", ".sha1", ".md5")))
        }, {
            "locale": "en-US",
            "task-id": {"task-reference": "<signing>"},
            "paths": publications_to_artifact_map_paths(name, version, publications, (".asc",))
        }]

        task["worker"]["artifact-map"] = artifact_map
        yield task


@transforms.add
def beetmover_task(config, tasks):
    for task in tasks:
        task["worker"]["max-run-time"] = 600
        task["worker"]["version"] = get_version()
        task["description"] = task["description"].format(task["attributes"]["buildconfig"]["name"])
        resolve_keyed_by(task, "worker.bucket", item_name=task["name"], **{
            "level": config.params["level"],
        })
        yield task


@transforms.add
def remove_dependent_tasks(config, tasks):
    for task in tasks:
        del task["primary-dependency"]
        del task["dependent-tasks"]
        yield task
