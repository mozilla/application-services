# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.


from taskgraph.transforms.base import TransformSequence
from taskgraph.util.schema import resolve_keyed_by

from ..build_config import get_version
from . import publications_to_artifact_paths

transforms = TransformSequence()


@transforms.add
def build_upstream_artifacts(config, tasks):
    for task in tasks:
        module_info = task["attributes"]["buildconfig"]
        version = get_version(config.params)

        worker_definition = {
            "upstream-artifacts": [
                {
                    "taskId": {
                        "task-reference": f"<{task['attributes']['primary-kind-dependency']}>"
                    },
                    "taskType": "build",
                    "paths": publications_to_artifact_paths(
                        version, module_info["publications"]
                    ),
                    "formats": ["gcp_prod_autograph_gpg"],
                }
            ]
        }

        task.setdefault("worker", {})
        task["worker"].update(worker_definition)
        yield task


@transforms.add
def signing_task(config, tasks):
    for task in tasks:
        task["description"] = task["description"].format(
            task["attributes"]["buildconfig"]["name"]
        )
        resolve_keyed_by(
            task,
            "worker.cert",
            item_name=task["name"],
            **{
                "level": config.params["level"],
            },
        )
        yield task
