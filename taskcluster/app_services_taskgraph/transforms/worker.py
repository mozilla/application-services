# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

from taskgraph.transforms.base import TransformSequence

transforms = TransformSequence()


@transforms.add
def setup_worker(_, tasks):
    for task in tasks:
        task_name = task["name"]
        try:
            worker_type = task["worker-type"]
        except KeyError:
            raise ValueError(f"worker-type not set for {task_name}")
        if worker_type == "b-linux":
            worker = task.setdefault("worker", {})
            worker["docker-image"] = {"in-tree": "linux"}
        elif worker_type == "b-osx":
            pass  # nothing to do here except avoid raising a ValueError
        else:
            raise ValueError(f"Unknown worker type for {task_name} ({worker_type})")
        yield task
