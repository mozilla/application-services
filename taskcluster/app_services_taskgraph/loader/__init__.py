# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

from __future__ import absolute_import, print_function, unicode_literals

import copy


# Define a collection of group_by functions
GROUP_BY_MAP = {}


def group_by(name):
    def wrapper(func):
        GROUP_BY_MAP[name] = func
        return func
    return wrapper


def group_tasks(config, tasks):
    group_by_fn = GROUP_BY_MAP[config['group-by']]

    groups = group_by_fn(config, tasks)

    for combinations in groups.itervalues():
        dependencies = [copy.deepcopy(t) for t in combinations]
        yield dependencies


@group_by('component')
def component_grouping(config, tasks):
    groups = {}
    for task in tasks:
        if task.kind not in config.get("kind-dependencies", []):
            continue

        buildconfig = task.attributes["buildconfig"]
        component = buildconfig["name"]
        if component == "all":
            continue

        groups.setdefault(component, []).append(task)

    tasks_for_all_components = [
        task for task in tasks
        if task.attributes.get("buildconfig", {}).get("name", "") == "all"
    ]
    for _, tasks in groups.iteritems():
        tasks.extend([
            task for task in tasks_for_all_components
        ])

    return groups
