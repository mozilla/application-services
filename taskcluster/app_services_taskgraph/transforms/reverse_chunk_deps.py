# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.
#
# Adds dependencies on all tasks from a kind referenced in `kind-dependencies`

from taskgraph.transforms.base import TransformSequence

transforms = TransformSequence()

@transforms.add
def add_dependencies(config, jobs):
    for job in jobs:
        job["dependencies"] = {}
        for dep_task in config.kind_dependencies_tasks.values():
            job["dependencies"][dep_task.label] = dep_task.label

        yield job
