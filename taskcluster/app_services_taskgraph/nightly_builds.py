# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

from taskgraph.filter_tasks import filter_task

@filter_task("nightly-build")
def filter_nightly_tasks(full_task_graph, parameters, graph_config):
    if parameters.get('preview-build') == 'nightly':
        # Nothing to filter for nightly builds
        return full_task_graph.tasks.keys()
    else:
        return [
            label
            for label, task in full_task_graph.tasks.items()
            if task.attributes.get('nightly') != 'nightly-only'
        ]
