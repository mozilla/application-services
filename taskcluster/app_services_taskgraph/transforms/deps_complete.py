# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

import itertools
from copy import deepcopy

from taskgraph import MAX_DEPENDENCIES
from taskgraph.transforms.base import TransformSequence

transforms = TransformSequence()
alerts = TransformSequence()


@transforms.add
def deps_complete_script(config, tasks):
    """Setup the deps-complete.py script"""
    for task in tasks:
        task.update(
            {
                # Run this task when all dependencies are completed, rather than
                # requiring them to be successful
                "requires": "all-resolved",
                "worker-type": "b-linux",
                "worker": {
                    "chain-of-trust": True,
                    "docker-image": {"in-tree": "linux"},
                    "max-run-time": 1800,
                    "env": {
                        "DECISION_TASK_ID": {"task-reference": "<decision>"},
                        "TASK_ID": {"task-reference": "<self>"},
                    },
                },
                "run": {
                    "using": "run-task",
                    "command": "/builds/worker/checkouts/vcs/taskcluster/scripts/deps-complete.py",
                },
            }
        )
        yield task


@transforms.add
def convert_dependencies(config, tasks):
    """
    Convert dependencies into soft-dependencies

    This means that taskcluster won't schedule the dependencies if only this
    task depends on them.
    """
    for task in tasks:
        task.setdefault("soft-dependencies", [])
        task["soft-dependencies"] += [
            dep_task.label for dep_task in config.kind_dependencies_tasks.values()
        ]
        yield task


@alerts.add
@transforms.add
def add_alert_routes(config, tasks):
    """
    Add routes to alert channels when this task fails.
    """
    for task in tasks:
        alerts = task.pop("alerts", {})
        if config.params["level"] != "3":
            yield task
            continue

        task.setdefault("routes", [])
        for name, value in alerts.items():
            if name not in ("slack-channel", "email", "pulse", "matrix-room"):
                raise KeyError(f"Unknown alert type: {name}")
            task["routes"].append(f"notify.{name}.{value}.on-failed")
        yield task


# Transform that adjusts the dependencies to not exceed MAX_DEPENDENCIES
#
# This transform checks if the dependency count exceeds MAX_DEPENDENCIES.  If
# so, it creates a child jobs with exactly MAX_DEPENDENCIES that the main task
# can then depend on.
#
# This is separated out from the main transform since it depends on
# taskgraph.transforms.run:transforms running first.
#
# This code is based off the reverse_chunk_deps transform from Gecko
reverse_chunk = TransformSequence()


def adjust_dependencies_child_job(orig_job, deps, count):
    job = deepcopy(orig_job)
    job["soft-dependencies"] = deps
    job["label"] = "{} - {}".format(orig_job["label"], count)
    del job["routes"]  # don't send alerts for child jobs
    return job


@reverse_chunk.add
def adjust_dependencies(config, jobs):
    for job in jobs:
        counter = itertools.count(1)
        # sort for deterministic chunking
        deps = sorted(job["soft-dependencies"])

        while len(deps) > MAX_DEPENDENCIES:
            # split off the first N deps
            chunk, deps = deps[:MAX_DEPENDENCIES], deps[MAX_DEPENDENCIES:]
            # create a separate job to handle them
            child_job = adjust_dependencies_child_job(job, chunk, next(counter))
            # Yield the child job and add a dependency to it
            yield child_job
            deps.append(child_job["label"])

        # Yield the parent job with the rest of the deps
        job["soft-dependencies"] = deps
        yield job
