# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

import os
import re

from taskgraph.filter_tasks import filter_task

ANDROID_COMPONENTS_BRANCH_RE = re.compile(r'\[ac:\s*([\w-]+)\]')
FENIX_BRANCH_RE = re.compile(r'\[fenix:\s*([\w-]+)\]')

def update_decision_parameters(parameters):
    parameters['branch-build'] = calc_branch_build_param()
    parameters['filters'].append('branch-build')

def calc_branch_build_param():
    title = os.environ.get("APPSERVICES_PULL_REQUEST_TITLE", "")
    branch_build = {}

    ac_branch_match = ANDROID_COMPONENTS_BRANCH_RE.search(title)
    if ac_branch_match:
        branch_build['android-components-branch'] = ac_branch_match.group(1)

    fenix_branch_match = FENIX_BRANCH_RE.search(title)
    if fenix_branch_match:
        branch_build['fenix-branch'] = fenix_branch_match.group(1)

    return branch_build

@filter_task("branch-build")
def filter_branch_build_tasks(full_task_graph, parameters, graph_config):
    if parameters.get('branch-build'):
        # If the branch_build param is set, don't filter anything
        return full_task_graph.tasks.keys()
    else:
        # If the branch_build param is unset, remove the branch-build tasks
        return [
            label
            for label, task in full_task_graph.tasks.items()
            if 'branch-build' not in task.attributes
        ]
