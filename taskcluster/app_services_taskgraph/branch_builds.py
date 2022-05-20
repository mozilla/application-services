# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

import os
import re

from taskgraph.filter_tasks import filter_task

REPO_RE = r'((?P<owner>[\.\w-]+)/)?(?P<branch>[\.\w-]+)'
ANDROID_COMPONENTS_BRANCH_RE = re.compile(r'\[a-?c:\s*' + REPO_RE + r'\]')
FENIX_BRANCH_RE = re.compile(r'\[fenix:\s*' + REPO_RE + r'\]')

def update_decision_parameters(parameters):
    parameters['branch-build'] = calc_branch_build_param(parameters)
    parameters['filters'].append('branch-build')

def calc_branch_build_param(parameters):
    if parameters.get('nightly-build'):
        return {
            'android-components-branch': 'main',
            'fenix-branch': 'main',
        }
    title = os.environ.get("APPSERVICES_PULL_REQUEST_TITLE", "")
    branch_build = {}

    ac_branch_match = ANDROID_COMPONENTS_BRANCH_RE.search(title)
    if ac_branch_match:
        branch_build['android-components-owner'] = calc_owner(ac_branch_match)
        branch_build['android-components-branch'] = ac_branch_match.group('branch')

    fenix_branch_match = FENIX_BRANCH_RE.search(title)
    if fenix_branch_match:
        branch_build['fenix-owner'] = calc_owner(fenix_branch_match)
        branch_build['fenix-branch'] = fenix_branch_match.group('branch')

    return branch_build

def calc_owner(match):
    if match.group('owner'):
        return match.group('owner')
    else:
        return 'mozilla-mobile'

@filter_task("branch-build")
def filter_branch_build_tasks(full_task_graph, parameters, graph_config):
    if parameters.get('branch-build'):
        # If the branch_build param is set, don't filter anything
        # Also don't filter for the nightly build.  At least for now, we can
        # assume all cron-triggered builds are nightlies.
        return full_task_graph.tasks.keys()
    else:
        # If the branch_build param is unset, remove the branch-build tasks
        return [
            label
            for label, task in full_task_graph.tasks.items()
            if 'branch-build' not in task.attributes
        ]
